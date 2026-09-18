package xyz.xenondevs.tessera.capture.dump

import com.mojang.blaze3d.vertex.PoseStack
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap
import net.minecraft.client.gui.Font
import net.minecraft.client.model.Model
import net.minecraft.client.model.geom.ModelPart
import net.minecraft.client.model.geom.builders.UVPair
import net.minecraft.client.renderer.OrderedSubmitNodeCollector
import net.minecraft.client.renderer.SubmitNodeCollector
import net.minecraft.client.renderer.block.MovingBlockRenderState
import net.minecraft.client.renderer.block.dispatch.BlockStateModelPart
import net.minecraft.client.renderer.entity.state.EntityRenderState
import net.minecraft.client.renderer.feature.ModelFeatureRenderer
import net.minecraft.client.renderer.gizmos.DrawableGizmoPrimitives
import net.minecraft.client.renderer.item.ItemStackRenderState
import net.minecraft.client.renderer.rendertype.RenderType
import net.minecraft.client.renderer.state.level.CameraRenderState
import net.minecraft.client.renderer.state.level.QuadParticleRenderState
import net.minecraft.client.renderer.texture.UvMapping
import net.minecraft.client.resources.model.geometry.ItemQuads
import net.minecraft.core.Direction
import net.minecraft.network.chat.Component
import net.minecraft.util.FormattedCharSequence
import net.minecraft.world.item.ItemDisplayContext
import net.minecraft.world.phys.Vec3
import net.minecraft.world.phys.shapes.VoxelShape
import org.joml.Matrix4f
import org.joml.Quaternionf
import org.joml.Vector2f
import org.joml.Vector2fc
import org.joml.Vector3f
import org.joml.Vector3fc
import java.util.*
import kotlin.math.max
import kotlin.math.min

private val CULL_FACES: List<Direction?> = Direction.entries + null

class TesseraCollector private constructor(
    private val sink: CaptureSink
) : SubmitNodeCollector, OrderedSubmitNodeCollector by OrderedCapture(sink, 0) {
    
    constructor() : this(CaptureSink())
    
    private val orders = Int2ObjectOpenHashMap<OrderedCapture>()
    
    override fun order(order: Int): OrderedSubmitNodeCollector =
        orders.getOrPut(order) { OrderedCapture(sink, order) }
    
    fun finish(): CaptureResult =
        if (sink.discards.isEmpty()) CaptureResult.Captured(sink.submissions)
        else CaptureResult.Discarded(sink.discards)
    
}

private class CaptureSink {
    
    val submissions = ArrayList<CapturedSubmission>()
    val discards: EnumSet<DiscardReason> = EnumSet.noneOf(DiscardReason::class.java)
    
    fun describe(read: ReadMaterial) = when (read) {
        is ReadMaterial.Described -> read.material
        is ReadMaterial.Undescribable -> {
            discards += read.reason
            null
        }
    }
}

private class OrderedCapture(private val sink: CaptureSink, private val order: Int) : OrderedSubmitNodeCollector {
    
    override fun <S : Any> submitModel(
        model: Model<in S>,
        state: S,
        poseStack: PoseStack,
        renderType: RenderType,
        lightCoords: Int,
        overlayCoords: Int,
        tintedColor: Int,
        uvMapping: UvMapping?,
        outlineColor: Int
    ) {
        val material = sink.describe(renderType.readMaterial(uvMapping, null, tintedColor.takeIf { it != -1 }, null, 0))
            ?: return
        
        model.setupAnim(state)
        
        fun captureCube(cube: ModelPart.Cube): CapturedCube? {
            val from = Vector3f(Float.POSITIVE_INFINITY)
            val to = Vector3f(Float.NEGATIVE_INFINITY)
            for (polygon in cube.polygons) {
                for (vertex in polygon.vertices) {
                    from.set(min(from.x, vertex.x), min(from.y, vertex.y), min(from.z, vertex.z))
                    to.set(max(to.x, vertex.x), max(to.y, vertex.y), max(to.z, vertex.z))
                }
            }
            
            val faces = EnumMap<Direction, List<Vector2fc>>(Direction::class.java)
            for (polygon in cube.polygons) {
                val direction = Direction.entries.first { it.unitVec3f == polygon.normal }
                val bit = direction.cornerAxisBit
                
                if ((bit != 0b001 && from.x == to.x || (bit != 0b010 && from.y == to.y) || (bit != 0b100 && from.z == to.z)))
                    continue
                val uvs = arrayOfNulls<Vector2fc>(8)
                for (vertex in polygon.vertices) {
                    var corner = if (direction.axisDirection == Direction.AxisDirection.POSITIVE) bit else 0
                    if (bit != 0b001 && vertex.x == to.x)
                        corner = corner or 0b001
                    if (bit != 0b010 && vertex.y == to.y)
                        corner = corner or 0b010
                    if (bit != 0b100 && vertex.z == to.z)
                        corner = corner or 0b100
                    uvs[corner] = Vector2f(vertex.u, vertex.v)
                }
                
                faces[direction] = uvs.filterNotNull()
            }
            
            return if (faces.isEmpty()) null else CapturedCube(from, to, faces)
        }
        
        val parts = ArrayList<CapturedPart>()
        val scratchPose = PoseStack()
        
        data class Frame(val part: ModelPart, val path: String, val parentPose: PoseStack.Pose)
        
        val pending = ArrayList<Frame>()
        pending += Frame(model.root(), "", PoseStack().last())
        
        while (pending.isNotEmpty()) {
            val (part, path, parentPose) = pending.removeLast()
            
            if (!part.visible) continue
            
            scratchPose.last().set(parentPose)
            part.translateAndRotate(scratchPose)
            val pose = scratchPose.last().copy()
            
            if (!part.skipDraw) {
                val cubes = part.cubes.mapNotNull(::captureCube)
                if (cubes.isNotEmpty()) {
                    val pixelPose = Matrix4f(pose.pose())
                    pixelPose.m30(pixelPose.m30() * 16f).m31(pixelPose.m31() * 16f).m32(pixelPose.m32() * 16f)
                    parts += CapturedPart(path, pixelPose, cubes)
                }
            }
            
            val index = pending.size
            for ((name, child) in part.children)
                pending.add(index, Frame(child, if (path.isEmpty()) name else "$path/$name", pose))
        }
        
        if (parts.isNotEmpty())
            sink.submissions += CapturedSubmission.Model(order, Matrix4f(poseStack.last().pose()), material, parts)
    }
    
    override fun submitBlockModel(
        poseStack: PoseStack,
        renderType: RenderType,
        parts: List<BlockStateModelPart>,
        tintLayers: IntArray,
        lightCoords: Int,
        overlayCoords: Int,
        outlineColor: Int
    ) {
        val quads = ArrayList<CapturedPrimitive>()
        
        fun addQuad(mat: CapturedMaterial, normal: Vector3fc, positions: List<Vector3fc>, uvs: List<Vector2fc>) {
            when (QuadShape.of(positions, uvs)) {
                QuadShape.PARALLELOGRAM -> quads += CapturedPrimitive.Parallelogram(mat, normal, positions, uvs)
                QuadShape.GENERAL -> {
                    quads += CapturedPrimitive.Triangle(mat, normal, positions.subList(0, 3), uvs.subList(0, 3))
                    quads += CapturedPrimitive.Triangle(mat, normal, listOf(positions[2], positions[3], positions[1]), listOf(uvs[2], uvs[3], uvs[1]))
                }
                
                QuadShape.INVISIBLE -> Unit
            }
        }
        
        for (part in parts) {
            for (face in CULL_FACES) {
                for (quad in part.getQuads(face)) {
                    val matInfo = quad.materialInfo
                    val sprite = matInfo.sprite
                    val tintIndex = matInfo.tintIndex.takeIf { it != -1 }
                    val tintColor = tintIndex?.let(tintLayers::getOrNull)
                    val mat = sink.describe(renderType.readMaterial(sprite, tintIndex, tintColor, matInfo.shadeDirectionOverride, matInfo.lightEmission))
                        ?: return
                    
                    val positions = List(4) { Vector3f(quad.position(it)) }
                    val uvs = List(4) {
                        val packed = quad.packedUV(it)
                        Vector2f(
                            (UVPair.unpackU(packed) - sprite.u0) / (sprite.u1 - sprite.u0),
                            (UVPair.unpackV(packed) - sprite.v0) / (sprite.v1 - sprite.v0)
                        )
                    }
                    addQuad(mat, quad.direction.unitVec3f, positions, uvs)
                }
            }
        }
        
        if (quads.isNotEmpty())
            sink.submissions += CapturedSubmission.BlockModel(order, Matrix4f(poseStack.last().pose()), quads)
    }
    
    override fun submitText(
        poseStack: PoseStack,
        x: Float,
        y: Float,
        string: FormattedCharSequence,
        dropShadow: Boolean,
        displayMode: Font.DisplayMode,
        lightCoords: Int,
        color: Int,
        backgroundColor: Int,
        outlineColor: Int
    ) {
        // TODO
        if (!string.accept { _, _, _ -> false })
            sink.discards += DiscardReason.TEXT
    }
    
    override fun submitMovingBlock(poseStack: PoseStack, movingBlockRenderState: MovingBlockRenderState, outlineColor: Int) {
        // TODO
        sink.discards += DiscardReason.MOVING_BLOCK
    }
    
    override fun submitBreakingBlockModel(poseStack: PoseStack, parts: List<BlockStateModelPart>, progress: Int, isBlockTranslucent: Boolean) {
        // TODO
        sink.discards += DiscardReason.BREAKING_BLOCK
    }
    
    override fun submitItem(poseStack: PoseStack, displayContext: ItemDisplayContext, lightCoords: Int, overlayCoords: Int, outlineColor: Int, tintLayers: IntArray, quads: ItemQuads, foilType: ItemStackRenderState.FoilType) {
        // TODO
        sink.discards += DiscardReason.ITEM
    }
    
    override fun submitCustomGeometry(poseStack: PoseStack, renderType: RenderType, customGeometryRenderer: SubmitNodeCollector.CustomGeometryRenderer) {
        // TODO
        sink.discards += DiscardReason.CUSTOM_GEOMETRY
    }
    
    override fun <S : Any> submitCrumblingOverlay(
        model: Model<in S>,
        state: S,
        poseStack: PoseStack,
        renderType: RenderType,
        lightCoords: Int,
        overlayCoords: Int,
        tintedColor: Int,
        crumblingOverlay: ModelFeatureRenderer.CrumblingOverlay
    ) = Unit
    
    override fun submitShadow(poseStack: PoseStack, radius: Float, pieces: List<EntityRenderState.ShadowPiece>) = Unit
    
    override fun submitNameTag(
        poseStack: PoseStack,
        nameTagAttachment: Vec3?,
        offset: Int,
        name: Component,
        seeThrough: Boolean,
        lightCoords: Int,
        camera: CameraRenderState
    ) = Unit
    
    override fun submitTextBackground(
        poseStack: PoseStack,
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
        color: Int,
        displayMode: Font.DisplayMode,
        lightCoords: Int
    ) = Unit
    
    override fun submitFlame(poseStack: PoseStack, renderState: EntityRenderState, rotation: Quaternionf) = Unit
    
    override fun submitLeash(poseStack: PoseStack, leashState: EntityRenderState.LeashState) = Unit
    
    override fun submitShapeOutline(
        poseStack: PoseStack,
        shape: VoxelShape,
        renderType: RenderType,
        color: Int,
        width: Float,
        afterTerrain: Boolean
    ) = Unit
    
    override fun submitQuadParticleGroup(particles: QuadParticleRenderState) = Unit
    
    override fun submitGizmoPrimitives(group: DrawableGizmoPrimitives.Group, camera: CameraRenderState, onTop: Boolean) = Unit
    
}

val Direction.cornerAxisBit
    get() = when (this.axis) {
        Direction.Axis.X -> 0b001
        Direction.Axis.Y -> 0b010
        Direction.Axis.Z -> 0b100
    }
