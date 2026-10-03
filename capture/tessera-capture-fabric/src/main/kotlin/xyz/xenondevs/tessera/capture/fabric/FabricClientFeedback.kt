package xyz.xenondevs.tessera.capture.fabric

import net.fabricmc.fabric.api.client.command.v2.FabricClientCommandSource
import net.kyori.adventure.text.Component
import xyz.xenondevs.tessera.capture.util.CommandFeedback
import xyz.xenondevs.tessera.capture.util.toVanilla

object FabricClientFeedback : CommandFeedback<FabricClientCommandSource> {
    override fun info(source: FabricClientCommandSource, message: Component) {
        source.sendFeedback(message.toVanilla())
    }
    
    override fun error(source: FabricClientCommandSource, message: Component) {
        source.sendError(message.toVanilla())
    }
}
