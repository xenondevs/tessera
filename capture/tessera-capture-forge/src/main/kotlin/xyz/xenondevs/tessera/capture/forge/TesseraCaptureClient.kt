package xyz.xenondevs.tessera.capture.forge

import ModConstants
import net.minecraft.client.Minecraft
import net.neoforged.api.distmarker.Dist
import net.neoforged.fml.common.Mod
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent
import net.neoforged.neoforge.client.event.RegisterClientCommandsEvent
import net.neoforged.neoforge.common.NeoForge
import xyz.xenondevs.tessera.capture.command.CaptureCommand
import xyz.xenondevs.tessera.capture.dump.Capturer

@Mod(ModConstants.MOD_ID, dist = [Dist.CLIENT])
class TesseraCaptureClient {
    
    init {
        NeoForge.EVENT_BUS.addListener<RegisterClientCommandsEvent> {
            CaptureCommand.register(it.dispatcher, it.buildContext, ForgeClientFeedback)
        }
        NeoForge.EVENT_BUS.addListener<ClientPlayerNetworkEvent.LoggingOut> {
            Minecraft.getInstance().execute(Capturer::reset)
        }
    }
    
}
