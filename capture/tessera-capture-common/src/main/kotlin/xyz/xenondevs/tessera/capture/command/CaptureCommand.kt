package xyz.xenondevs.tessera.capture.command

import com.mojang.brigadier.CommandDispatcher
import com.mojang.brigadier.context.CommandContext
import net.kyori.adventure.text.Component
import net.kyori.adventure.text.event.ClickEvent
import net.kyori.adventure.text.format.TextDecoration
import net.minecraft.SharedConstants
import net.minecraft.client.Minecraft
import net.minecraft.commands.CommandBuildContext
import net.minecraft.commands.arguments.blocks.BlockInput
import net.minecraft.commands.arguments.blocks.BlockPredicateArgument
import net.minecraft.commands.arguments.blocks.BlockStateArgument
import net.minecraft.core.registries.BuiltInRegistries
import net.minecraft.world.phys.BlockHitResult
import net.minecraft.world.phys.HitResult
import xyz.xenondevs.tessera.capture.dump.CaptureResult
import xyz.xenondevs.tessera.capture.dump.Capturer
import xyz.xenondevs.tessera.capture.dump.DiscardReason
import xyz.xenondevs.tessera.capture.dump.sink.BlockRecord
import xyz.xenondevs.tessera.capture.dump.sink.RawRecordWriter
import xyz.xenondevs.tessera.capture.dump.sink.StateRecord
import xyz.xenondevs.tessera.capture.util.CommandFeedback
import xyz.xenondevs.tessera.capture.util.argument
import xyz.xenondevs.tessera.capture.util.literal
import xyz.xenondevs.tessera.capture.util.runs
import java.nio.file.Path
import java.util.*
import kotlin.io.path.ExperimentalPathApi
import kotlin.io.path.createParentDirectories
import kotlin.io.path.deleteRecursively
import kotlin.io.path.writeText

object CaptureCommand {
    
    fun <S : Any> register(dispatcher: CommandDispatcher<S>, ctx: CommandBuildContext, fb: CommandFeedback<S>) {
        dispatcher.register(
            literal<S>("tcap")
                .then(
                    literal<S>("embed")
                        .runs { dumpEmbed(it, fb) }
                )
                .then(
                    literal<S>("block")
                        .runs { renderTargetBlock(it, fb) }
                        .then(
                            argument<S, BlockInput>("block", BlockStateArgument.block(ctx))
                                .runs { dumpBlockState(it, fb) }
                        )
                        .then(
                            argument<S, BlockPredicateArgument.Result>("blocks", BlockPredicateArgument.blockPredicate(ctx))
                                .runs { dumpBlockStates(it, fb) }
                        )
                )
        )
    }
    
    private fun <S : Any> renderTargetBlock(ctx: CommandContext<S>, fb: CommandFeedback<S>) {
        val client = Minecraft.getInstance()
        val source = ctx.source
        
        if (client.hitResult?.type != HitResult.Type.BLOCK) {
            fb.error(source, Component.text("You are not looking at a block!"))
            return
        }
        
        val hitPos = (client.hitResult as? BlockHitResult)?.blockPos
        
        if (hitPos == null) {
            fb.error(source, Component.text("You are not looking at a block!"))
            return
        }
        
        fb.info(source, Component.text("Capturing targeted block"))
        // TODO
    }
    
    private fun <S : Any> dumpBlockState(ctx: CommandContext<S>, fb: CommandFeedback<S>) {
        val source = ctx.source
        
        val blockState = ctx.getArgument("block", BlockInput::class.java).state
        
        when (val result = Capturer.capture(blockState)) {
            is CaptureResult.Captured -> {
//                val report = DebugModelDumper.dump(blockState, result.submissions)
//                fb.info(source, Component.text("Wrote ${report.elements} elements to ")
//                    .append(diskLink(report.file.parent)))
//                if (report.skipped.isNotEmpty())
//                    fb.error(source, Component.text("Skipped ${report.skipped}"))
            }
            
            is CaptureResult.Discarded -> fb.error(source, Component.text("Discarded: ${result.reasons}"))
            
            CaptureResult.NoBlockEntityRenderer -> fb.info(source, Component.text("$blockState has no block entity renderer"))
        }
    }
    
    private fun <S : Any> dumpBlockStates(ctx: CommandContext<S>, fb: CommandFeedback<S>) {
        val source = ctx.source
        
        val predicate = ctx.getArgument("blocks", BlockPredicateArgument.Result::class.java)
        
        fb.info(source, Component.text("Capturing block state(s) matching predicate"))
        // TODO
    }
    
    @OptIn(ExperimentalPathApi::class)
    private fun <S : Any> dumpEmbed(ctx: CommandContext<S>, fb: CommandFeedback<S>) {
        val directory = Minecraft.getInstance().gameDirectory.toPath().resolve("tessera-capture").resolve("embed")
        directory.deleteRecursively()
        
        val minecraftVer = SharedConstants.getCurrentVersion().id()
        var captured = 0
        var capturedEmpty = 0
        var noRenderer = 0
        var discarded = 0
        val discardReasons = EnumMap<DiscardReason, Int>(DiscardReason::class.java)
        
        for (block in BuiltInRegistries.BLOCK) {
            val id = BuiltInRegistries.BLOCK.getKey(block)
            val states = block.stateDefinition.possibleStates.map(StateRecord::capture)
            for ((_, _, result) in states) {
                when (result) {
                    CaptureResult.NoBlockEntityRenderer -> ++noRenderer
                    is CaptureResult.Captured -> if (result.submissions.isEmpty()) ++capturedEmpty else ++captured
                    is CaptureResult.Discarded -> {
                        ++discarded
                        for (reason in result.reasons)
                            discardReasons.merge(reason, 1) { a, b -> a + b }
                    }
                }
            }
            if (states.none(StateRecord::isNeeded)) continue
            
            val file = directory.resolve(id.namespace).resolve("${id.path}.json")
            file.createParentDirectories()
            file.writeText(RawRecordWriter.write(BlockRecord(id, states), minecraftVer))
        }
        val summary = "Captured $captured states, $capturedEmpty empty, $noRenderer with no renderer, $discarded discarded. Wrote "
        fb.info(ctx.source, Component.text(summary).append(diskLink(directory)))
        if (discardReasons.isNotEmpty())
            fb.error(ctx.source, Component.text("Discard reasons: $discardReasons"))
    }
    
    private fun diskLink(path: Path) = Component.text("[disk]")
        .decoration(TextDecoration.BOLD, true)
        .clickEvent(ClickEvent.openFile(path.toString()))
    
}
