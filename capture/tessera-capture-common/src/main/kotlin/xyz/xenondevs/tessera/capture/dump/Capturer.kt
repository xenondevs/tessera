package xyz.xenondevs.tessera.capture.dump

import com.mojang.blaze3d.vertex.PoseStack
import com.mojang.logging.LogUtils
import net.minecraft.client.Minecraft
import net.minecraft.client.renderer.blockentity.state.BlockEntityRenderState
import net.minecraft.client.renderer.state.level.CameraRenderState
import net.minecraft.core.BlockPos
import net.minecraft.world.level.block.EntityBlock
import net.minecraft.world.level.block.entity.BlockEntity
import net.minecraft.world.level.block.state.BlockState
import net.minecraft.world.phys.Vec3
import xyz.xenondevs.tessera.capture.util.WorldArea
import java.util.*

object Capturer {
    
    private val LOGGER = LogUtils.getLogger()
    
    private var captureLevel: CaptureLevel? = null
    
    fun capture(state: BlockState): CaptureResult {
        val block = state.block as? EntityBlock
            ?: return CaptureResult.NoBlockEntityRenderer
        val blockEntity = block.newBlockEntity(BlockPos.ZERO, state)
            ?: return CaptureResult.NoBlockEntityRenderer
        val client = Minecraft.getInstance()
        val renderer = client.blockEntityRenderDispatcher.getRenderer<BlockEntity, BlockEntityRenderState>(blockEntity)
            ?: return CaptureResult.NoBlockEntityRenderer
        prepLevel(WorldArea.newSingleBlockArea(state, blockEntity))
        
        val collector = TesseraCollector()
        try {
            val renderState = renderer.createRenderState()
            renderer.extractRenderState(blockEntity, renderState, 0f, Vec3.ZERO, null)
            renderer.submit(renderState, PoseStack(), collector, CameraRenderState())
        }catch (e: Exception) {
            LOGGER.error("Error while capturing block entity", e)
            return CaptureResult.Discarded(EnumSet.of(DiscardReason.EXCEPTION))
        }
        
        return collector.finish()
    }
    
    private fun prepLevel(area: WorldArea): CaptureLevel {
        val conn = Minecraft.getInstance().connection
            ?: throw IllegalStateException("no connection")
        val access = conn.registryAccess()
        val level = captureLevel?.takeIf { it.registryAccess() === access }
            ?: CaptureLevel(access, conn.enabledFeatures(), area).also { captureLevel = it }
        level.area = area
        
        return level
    }
    
    fun reset() {
        captureLevel = null
    }
    
}
