use super::lookup::Capture;
use crate::resource::item::raw::EndCubeEffect;
use tessera_capture::model::{Draw, DrawGeometry, Effect, Pose};

pub fn end_cube(effect: EndCubeEffect) -> Capture<'static> {
    const SKY: &str = "minecraft:environment/end_sky";
    const PORTAL: &str = "minecraft:entity/end_portal/end_portal";
    static IDENTITY: Pose = Pose([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]);

    // first three corners of each face
    static CUBE: [[[f32; 3]; 3]; 6] = [
        [[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0]],
        [[1.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        [[0.0, 1.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0]],
        [[0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [[1.0, 1.0, 1.0], [1.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
    ];

    macro_rules! draws {
        ($layers:literal) => {{
            static EFFECT: Effect = Effect::EndPortal { layers: $layers, sky: SKY, portal: PORTAL };
            static DRAWS: [Draw; 1] = [Draw {
                order: 0,
                pose: &IDENTITY,
                geometry: DrawGeometry::Effect { effect: &EFFECT, quads: &CUBE },
            }];
            &DRAWS
        }};
    }

    let draws: &'static [Draw<'static>] = match effect {
        EndCubeEffect::Portal => draws!(15),
        EndCubeEffect::Gateway => draws!(16),
    };
    Capture { materials: &[], draws }
}
