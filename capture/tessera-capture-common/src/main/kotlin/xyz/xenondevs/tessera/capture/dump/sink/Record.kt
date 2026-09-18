package xyz.xenondevs.tessera.capture.dump.sink

import net.minecraft.resources.Identifier
import net.minecraft.world.level.block.RenderShape
import net.minecraft.world.level.block.state.BlockState
import xyz.xenondevs.tessera.capture.dump.CaptureResult
import xyz.xenondevs.tessera.capture.dump.Capturer
import kotlin.streams.asSequence

data class BlockRecord(val block: Identifier, val states: List<StateRecord>)

data class StateRecord(val key: String, val renderShape: RenderShape, val result: CaptureResult) {
    
    val isNeeded: Boolean
        get() = renderShape != RenderShape.MODEL || when (result) {
            CaptureResult.NoBlockEntityRenderer -> false
            is CaptureResult.Captured -> result.submissions.isNotEmpty()
            is CaptureResult.Discarded -> true
        }
    
    companion object {
        
        fun capture(state: BlockState) =
            StateRecord(stateKey(state), state.renderShape, Capturer.capture(state))
        
        fun stateKey(state: BlockState) =
            state.values.asSequence().joinToString(",") { "${it.property.name}=${it.valueName()}" }
    }
    
}
