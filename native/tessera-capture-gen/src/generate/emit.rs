use super::pool::{DrawGeometry, FloatingBits, Pools};
use crate::model::{Direction, Layering, QuadKind, RenderShape};
use proc_macro2::{Ident, Literal, TokenStream};
use quote::{ToTokens, format_ident, quote};

impl ToTokens for FloatingBits {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let value = self.get();
        if value.is_sign_negative() {
            let mag = Literal::f32_unsuffixed(-value);
            quote!(-#mag).to_tokens(tokens)
        } else {
            Literal::f32_unsuffixed(value).to_tokens(tokens)
        }
    }
}

macro_rules! enum_tokens {
    ($($name:ident),*) => {
        $(
            impl ToTokens for $name {
                fn to_tokens(&self, tokens: &mut TokenStream) {
                    let variant = format_ident!("{self:?}");
                    quote!($name::#variant).to_tokens(tokens);
                }
            }
        )*
    };
}

enum_tokens!(Direction, Layering, QuadKind, RenderShape);

pub(super) fn emit(pools: &Pools) -> String {
    fn static_name(category: &str, idx: usize) -> Ident {
        format_ident!("{category}_{idx}")
    }

    fn option<T: ToTokens>(value: Option<T>) -> TokenStream {
        match value {
            Some(value) => quote!(Some(#value)),
            None => quote!(None),
        }
    }

    fn int(value: i32) -> TokenStream {
        let mag = Literal::u32_unsuffixed(value.unsigned_abs());
        if value < 0 { quote!(-#mag) } else { quote!(#mag) }
    }

    fn runs<T>(category: &str, element: TokenStream, pool: &[Vec<T>], item: impl Fn(&T) -> TokenStream) -> TokenStream {
        let statics = pool.iter().enumerate().map(|(i, run)| {
            let name = static_name(category, i);
            let len = Literal::usize_unsuffixed(run.len());
            let items = run.iter().map(&item);
            quote! { static #name: [#element; #len] = [#(#items),*]; }
        });
        quote!(#(#statics)*)
    }

    let poses = pools.poses.items().iter().enumerate().map(|(i, pose)| {
        let name = static_name("POSE", i);
        quote! { static #name: Pose = Pose([#(#pose),*]); }
    });

    let faces = runs("FACES", quote!(Face), pools.faces.items(), |face| {
        let dir = face.direction;
        let uvs = face.uvs.iter().map(|uv| quote!([#(#uv),*]));
        quote!(Face { direction: #dir, uvs: [#(#uvs),*] })
    });

    let cubes = runs("CUBES", quote!(Cube<'static>), pools.cubes.items(), |cube| {
        let (from, to, faces) = (&cube.from, &cube.to, static_name("FACES", cube.faces));
        quote!(Cube { from: [#(#from),*], to: [#(#to),*], faces: &#faces })
    });

    let parts = runs("PARTS", quote!(Part<'static>), pools.parts.items(), |part| {
        let (pose, cubes) = (static_name("POSE", part.pose), static_name("CUBES", part.cubes));
        quote!(Part { pose: &#pose, cubes: &#cubes })
    });

    let quads = runs("QUADS", quote!(Quad), pools.quads.items(), |quad| {
        let (mat, kind) = (Literal::u8_unsuffixed(quad.material), quad.kind);
        let positions = quad.positions.iter().map(|pos| quote!([#(#pos),*]));
        let uvs = quad.uvs.iter().map(|uv| quote!([#(#uv),*]));
        quote!(Quad { material: #mat, kind: #kind, positions: [#(#positions),*], uvs: [#(#uvs),*] })
    });

    let materials = pools.materials.items().iter().enumerate().map(|(i, mat)| {
        let name = static_name("MATERIAL", i);
        let (tex, pipeline) = (&mat.texture, &mat.pipeline);
        let (cull, blend, depth_write, layering) = (mat.cull, mat.blend, mat.depth_write, mat.layering);
        let alpha_cutout = option(mat.alpha_cutout);
        let depth_bias = option(
            mat.depth_bias
                .map(|[scale, constant]| quote!(DepthBias { scale: #scale, constant: #constant })),
        );
        let tint_index = option(mat.tint_index.map(Literal::u8_unsuffixed));
        let tint_color = option(
            mat.tint_color
                .map(|color| format!("{color:#010X}").parse::<Literal>().expect("valid hex int literal")),
        );
        let shade = option(mat.shade);
        let light_emission = Literal::u8_unsuffixed(mat.light_emission);
        quote! {
            static #name: Material<'static> = Material {
                texture: #tex,
                pipeline: #pipeline,
                cull: #cull,
                blend: #blend,
                depth_write: #depth_write,
                alpha_cutout: #alpha_cutout,
                depth_bias: #depth_bias,
                layering: #layering,
                tint_index: #tint_index,
                tint_color: #tint_color,
                shade: #shade,
                light_emission: #light_emission,
            };
        }
    });

    let material_lists = runs(
        "MATERIALS",
        quote!(&'static Material<'static>),
        pools.material_lists.items(),
        |&material| {
            let material = static_name("MATERIAL", material);
            quote!(&#material)
        },
    );

    let draws = runs("DRAWS", quote!(Draw<'static>), pools.draws.items(), |draw| {
        let (order, pose) = (int(draw.order), static_name("POSE", draw.pose));
        let geometry = match draw.geometry {
            DrawGeometry::Model { material, parts } => {
                let (mat, parts) = (Literal::u8_unsuffixed(material), static_name("PARTS", parts));
                quote!(DrawGeometry::Model { material: #mat, parts: &#parts })
            }
            DrawGeometry::BlockModel { quads } => {
                let quads = static_name("QUADS", quads);
                quote!(DrawGeometry::BlockModel { quads: &#quads })
            }
        };
        quote!(Draw { order: #order, pose: &#pose, geometry: #geometry })
    });

    let states = runs("STATES", quote!(State<'static>), pools.states.items(), |state| {
        let (key, render_shape, draws) = (&state.key, state.render_shape, static_name("DRAWS", state.draws));
        quote!(State { key: #key, render_shape: #render_shape, draws: &#draws })
    });

    let properties = runs(
        "PROPERTIES",
        quote!(&'static str),
        pools.property_lists.items(),
        |property| quote!(#property),
    );

    let blocks = pools.blocks.iter().enumerate().map(|(i, block)| {
        let name = static_name("BLOCK", i);
        let (namespace, path) = (&block.namespace, &block.path);
        let properties = static_name("PROPERTIES", block.properties);
        let states = static_name("STATES", block.states);
        let materials = static_name("MATERIALS", block.materials);
        quote! {
            static #name: Block<'static> = Block {
                namespace: #namespace,
                path: #path,
                properties: &#properties,
                states: &#states,
                materials: &#materials,
            };
        }
    });
    let block_names = (0..pools.blocks.len()).map(|i| static_name("BLOCK", i));
    let block_count = Literal::usize_unsuffixed(pools.blocks.len());

    let tokens = quote! {
        use tessera_capture_gen::model::{
            Block, Cube, DepthBias, Direction, Draw, DrawGeometry, Face, Layering, Material, Part, Pose, Quad, QuadKind,
            RenderShape, State
        };

        #(#poses)*
        #faces
        #cubes
        #parts
        #quads
        #(#materials)*
        #material_lists
        #draws
        #states
        #properties
        #(#blocks)*

        pub static BLOCKS: [&Block<'static>; #block_count] = [#(&#block_names),*];
    };

    let file: syn::File = syn::parse2(tokens).expect("valid emit");
    format!("// Auto generated. Don't edit manually\n\n{}", prettyplease::unparse(&file))
}
