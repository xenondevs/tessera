use crate::encode;
use crate::encode::Bounds;
use crate::ffi::guard;
use crate::renderer::NativeRenderer;
use std::ffi::{CString, c_char};
use std::sync::Arc;
use std::{panic, ptr, slice};
use tessera::resource::ResourceId;
use tessera::scene::{FlatResolution, Framing, RenderError};
use tokio::task::JoinSet;

#[repr(C)]
pub struct RenderRequest {
    /// `namespace:path` in utf-8
    id: *const u8,
    id_len: usize,
    /// Blockstate props or `null` for items and models
    props: *const u8,
    props_len: usize,
    size: u32,
    margin: f32,
    kind: u8,
    framing: u8,
    encode: u8,
    /// Only read for items
    flat: u8,
}

const KIND_ITEM: u8 = 0;
const KIND_MODEL: u8 = 1;
const KIND_BLOCK_STATE: u8 = 2;
const FRAMING_GUI: u8 = 0;
const FRAMING_FIT: u8 = 1;
const FLAT_REQUESTED: u8 = 0;
const FLAT_NATIVE: u8 = 1;

enum Request {
    Item { id: ResourceId, flat: FlatResolution },
    Model(ResourceId),
    BlockState { id: ResourceId, props: String },
}

struct OutputOptions {
    size: u32,
    framing: Framing,
    encode: bool,
}

const STATUS_OK: i32 = 0;
const STATUS_EMPTY: i32 = 1;
const STATUS_INVALID_REQUEST: i32 = 2;
const STATUS_NO_VARIANT: i32 = 3;
const STATUS_NOT_PRIMED: i32 = 4;

type ResultFn = unsafe extern "C" fn(
    index: usize,
    status: i32,
    image: *const u8,
    image_len: usize,
    width: u32,
    height: u32,
    bounds: *const Bounds,
    error: *const c_char,
) -> i32;

struct Failure {
    status: i32,
    message: String,
}

impl Failure {
    fn invalid_request(message: String) -> Self {
        Self { status: STATUS_INVALID_REQUEST, message }
    }
}

impl From<RenderError> for Failure {
    fn from(err: RenderError) -> Self {
        let status = match err {
            RenderError::NoGeometry(_) => STATUS_EMPTY,
            RenderError::NoVariant(..) => STATUS_NO_VARIANT,
            RenderError::NotPrimed(_) => STATUS_NOT_PRIMED,
        };
        Self { status, message: err.to_string() }
    }
}

impl RenderRequest {
    unsafe fn decode(&self) -> Result<(Request, OutputOptions), String> {
        if self.size == 0 {
            return Err("Render size must be positive".to_string());
        }
        // SAFETY: JVM passes a live segment of len bytes. Arena::allocateFrom encodes to utf-8
        let id: ResourceId = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.id, self.id_len)) }
            .parse()
            .map_err(|err| format!("{err}"))?;
        let encode = self.encode != 0;
        let framing = match self.framing {
            FRAMING_GUI => Framing::Gui,
            FRAMING_FIT => Framing::Fit { margin: self.margin },
            _ => return Err(format!("Invalid framing value: {}", self.framing)),
        };
        let request = match self.kind {
            KIND_ITEM => {
                let flat = match self.flat {
                    FLAT_REQUESTED => FlatResolution::Requested,
                    FLAT_NATIVE => FlatResolution::Native,
                    _ => return Err(format!("Invalid flat resolution value: {}", self.flat)),
                };
                Request::Item { id, flat }
            }
            KIND_MODEL => Request::Model(id),
            KIND_BLOCK_STATE => {
                let props = if self.props.is_null() {
                    String::new()
                } else {
                    // SAFETY: JVM passes a live segment of len bytes. Arena::allocateFrom encodes to utf-8
                    unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.props, self.props_len)) }.to_string()
                };
                Request::BlockState { id, props }
            }
            _ => return Err(format!("Invalid request kind: {}", self.kind)),
        };

        Ok((request, OutputOptions { size: self.size, framing, encode }))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_render(
    renderer: *const NativeRenderer,
    requests: *const RenderRequest,
    count: usize,
    on_result: ResultFn,
) -> i32 {
    struct Rendered {
        data: Vec<u8>,
        width: u32,
        height: u32,
        bounds: Option<Bounds>,
    }

    fn deliver(on_result: ResultFn, index: usize, result: Result<Rendered, Failure>) -> i32 {
        // SAFETY: stub lives in the jvm renderer arena which is valid during this run
        match result {
            Ok(Rendered { data, width, height, bounds }) => {
                let bounds = bounds.as_ref().map_or(ptr::null(), ptr::from_ref);
                unsafe {
                    on_result(
                        index,
                        STATUS_OK,
                        data.as_ptr(),
                        data.len(),
                        width,
                        height,
                        bounds,
                        ptr::null(),
                    )
                }
            }
            Err(Failure { status: STATUS_EMPTY, .. }) => unsafe {
                on_result(index, STATUS_EMPTY, ptr::null(), 0, 0, 0, ptr::null(), ptr::null())
            },
            Err(Failure { status, message }) => {
                let msg = CString::new(message.replace('\0', "\\0")).expect("\\0 was just replaced");
                unsafe { on_result(index, status, ptr::null(), 0, 0, 0, ptr::null(), msg.as_ptr()) }
            }
        }
    }

    guard(-1, || {
        // SAFETY: the renderer came from tessera_create_renderer and the JVM keeps it alive for the call
        let renderer = unsafe { &*renderer };
        // SAFETY: Java passes an arena array of count requests that stays alive for this call
        let requests = unsafe { slice::from_raw_parts(requests, count) };

        renderer.runtime.block_on(async {
            let mut tasks = JoinSet::new();
            for (index, req) in requests.iter().enumerate() {
                // SAFETY: JVM passes strings encoded to utf-8 via Arena::allocateFrom
                match unsafe { req.decode() } {
                    Ok((req, output_options)) => {
                        let OutputOptions { size, framing, encode: wants_encode } = output_options;
                        let renderer = Arc::clone(&renderer.inner);
                        tasks.spawn(async move {
                            let image = match req {
                                Request::Item { id, flat } => renderer.render_item(&id, size, framing, flat).await,
                                Request::Model(id) => renderer.render_model(&id, size, framing).await,
                                Request::BlockState { id, props } => renderer.render_blockstate(&id, &props, size, framing).await,
                            };

                            let image = match image {
                                Ok(img) => img,
                                Err(err) => return (index, Err(Failure::from(err))),
                            };

                            let (width, height) = image.dimensions();
                            let (data, bounds) = if wants_encode {
                                encode::encode(&image)
                            } else {
                                let bounds = encode::find_bounds(&image, None);
                                (image.into_raw(), bounds)
                            };

                            (index, Ok(Rendered { data, width, height, bounds }))
                        });
                    }
                    Err(msg) => {
                        if deliver(on_result, index, Err(Failure::invalid_request(msg))) != 0 {
                            return 1;
                        }
                    }
                }
            }
            while let Some(joined) = tasks.join_next().await {
                let (index, res) = joined.unwrap_or_else(|err| panic::resume_unwind(err.into_panic()));
                if deliver(on_result, index, res) != 0 {
                    return 1; // drops tasks and aborts whatever is left
                }
            }
            0
        })
    })
}
