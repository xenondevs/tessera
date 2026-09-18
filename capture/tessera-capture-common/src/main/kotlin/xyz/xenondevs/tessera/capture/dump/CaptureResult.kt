package xyz.xenondevs.tessera.capture.dump

import net.minecraft.core.Direction
import net.minecraft.resources.Identifier
import org.joml.Matrix4fc
import org.joml.Vector2fc
import org.joml.Vector3f
import org.joml.Vector3fc

sealed interface CaptureResult {
    
    data object NoBlockEntityRenderer : CaptureResult
    
    data class Captured(val submissions: List<CapturedSubmission>): CaptureResult
    
    data class Discarded(val reasons: Set<DiscardReason>) : CaptureResult
    
}

sealed interface CapturedSubmission {
    val order: Int
    val pose: Matrix4fc
    
    data class Model(
        override val order: Int,
        override val pose: Matrix4fc,
        val material: CapturedMaterial,
        val parts: List<CapturedPart>
    ) : CapturedSubmission
    
    data class BlockModel(
        override val order: Int,
        override val pose: Matrix4fc,
        val quads: List<CapturedPrimitive>
    ) : CapturedSubmission
}

data class CapturedPart(val path: String, val pose: Matrix4fc, val cubes: List<CapturedCube>)

data class CapturedCube(val from: Vector3f, val to: Vector3f, val faces: Map<Direction, List<Vector2fc>>)

sealed interface CapturedTexture {
    data class Sprite(val id: Identifier): CapturedTexture
    
    data class Standalone(val path: Identifier): CapturedTexture
}

enum class Layering {
    NONE,
    VIEW_OFFSET_Z,
    VIEW_OFFSET_Z_FORWARD,
}

data class DepthBias(val scale: Float, val constant: Float)

data class CapturedMaterial(
    val texture: CapturedTexture,
    val pipeline: Identifier,
    val cull: Boolean,
    val blend: Boolean,
    val alphaCutout: Float?,
    val depthWrite: Boolean,
    val depthBias: DepthBias?,
    val layering: Layering,
    val tintIndex: Int?,
    val tintColor: Int?,
    val shade: Direction?,
    val lightEmission: Int
)

sealed interface CapturedPrimitive {
    
    val material: CapturedMaterial
    val normal: Vector3fc
    val positions: List<Vector3fc>
    val uvs: List<Vector2fc>
    
    data class Parallelogram(
        override val material: CapturedMaterial,
        override val normal: Vector3fc,
        override val positions: List<Vector3fc>,
        override val uvs: List<Vector2fc>
    ) : CapturedPrimitive
    
    data class Triangle(
        override val material: CapturedMaterial,
        override val normal: Vector3fc,
        override val positions: List<Vector3fc>,
        override val uvs: List<Vector2fc>
    ) : CapturedPrimitive
}

enum class DiscardReason {
    UNMAPPED_MODEL,
    UNSUPPORTED_RENDER_SETUP,
    CUSTOM_GEOMETRY,
    ITEM,
    MOVING_BLOCK,
    BREAKING_BLOCK,
    TEXT,
    EXCEPTION
}
