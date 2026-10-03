package xyz.xenondevs.tessera.capture.forge

import net.kyori.adventure.text.Component
import net.minecraft.commands.CommandSourceStack
import xyz.xenondevs.tessera.capture.util.CommandFeedback
import xyz.xenondevs.tessera.capture.util.toVanilla

object ForgeClientFeedback : CommandFeedback<CommandSourceStack> {
    override fun info(source: CommandSourceStack, message: Component) {
        source.sendSystemMessage(message.toVanilla())
    }
    
    override fun error(source: CommandSourceStack, message: Component) {
        source.sendFailure(message.toVanilla())
    }
    
}
