use crate::ffi::guard;
use crate::renderer::NativeRenderer;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use std::ffi::{CString, c_char};
use std::sync::Arc;
use std::{panic, ptr, slice};
use tessera::resource::ResourceId;
use tessera::scene::Framing;
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
}

const KIND_ITEM: u8 = 0;
const KIND_MODEL: u8 = 1;
const KIND_BLOCK_STATE: u8 = 2;
const FRAMING_GUI: u8 = 0;
const FRAMING_FIT: u8 = 1;

enum Request {
    Item(ResourceId),
    Model(ResourceId),
    BlockState {
        id: ResourceId,
        props: String,
        framing: Framing,
    },
}

type ResultFn = unsafe extern "C" fn(index: usize, png: *const u8, png_len: usize, error: *const c_char) -> i32;

impl RenderRequest {
    unsafe fn decode(&self) -> Result<(Request, u32), String> {
        if self.size == 0 {
            return Err("Render size must be positive".to_string());
        }
        // SAFETY: JVM passes a live segment of len bytes. Arena::allocateFrom encodes to utf-8
        let id: ResourceId = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.id, self.id_len)) }
            .parse()
            .map_err(|err| format!("{err}"))?;
        let request = match self.kind {
            KIND_ITEM => Request::Item(id),
            KIND_MODEL => Request::Model(id),
            KIND_BLOCK_STATE => {
                let props = if self.props.is_null() {
                    String::new()
                } else {
                    // SAFETY: JVM passes a live segment of len bytes. Arena::allocateFrom encodes to utf-8
                    unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.props, self.props_len)) }.to_string()
                };
                let framing = match self.framing {
                    FRAMING_GUI => Framing::Gui,
                    FRAMING_FIT => Framing::Fit { margin: self.margin },
                    _ => return Err(format!("Invalid framing value: {}", self.framing)),
                };
                Request::BlockState { id, props, framing }
            }
            _ => return Err(format!("Invalid request kind: {}", self.kind)),
        };

        Ok((request, self.size))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_render(
    renderer: *const NativeRenderer,
    requests: *const RenderRequest,
    count: usize,
    on_result: ResultFn,
) -> i32 {
    fn deliver(on_result: ResultFn, index: usize, result: Result<Vec<u8>, String>) -> i32 {
        // SAFETY: stub lives in the jvm renderer arena which is valid during this run
        match result {
            Ok(png) => unsafe { on_result(index, png.as_ptr(), png.len(), ptr::null()) },
            Err(msg) => {
                let msg = CString::new(msg.replace('\0', "\\0")).expect("\\0 was just replaced");
                unsafe { on_result(index, ptr::null(), 0, msg.as_ptr()) }
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
                    Ok((req, size)) => {
                        let renderer = Arc::clone(&renderer.inner);
                        tasks.spawn(async move {
                            let image = match req {
                                Request::Item(id) => renderer.render_item(&id, size).await,
                                Request::Model(id) => renderer.render_model(&id, size).await,
                                Request::BlockState { id, props, framing } => {
                                    renderer.render_blockstate(&id, &props, size, framing).await
                                }
                            }
                            .map_err(|e| e.to_string());
                            let png = image.and_then(|img| {
                                let mut png = Vec::new();
                                PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
                                    .write_image(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgba8)
                                    .map_err(|e| e.to_string())?;
                                Ok(png)
                            });

                            (index, png)
                        });
                    }
                    Err(msg) => {
                        if deliver(on_result, index, Err(msg)) != 0 {
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
