package xyz.xenondevs.tessera.capture.fabric

import net.fabricmc.api.ClientModInitializer
import net.fabricmc.fabric.api.client.command.v2.ClientCommandRegistrationCallback
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents
import xyz.xenondevs.tessera.capture.command.CaptureCommand
import xyz.xenondevs.tessera.capture.dump.Capturer

class TesseraCaptureClient : ClientModInitializer {
    override fun onInitializeClient() {
        ClientCommandRegistrationCallback.EVENT.register { dispatcher, ctx ->
            CaptureCommand.register(dispatcher, ctx, FabricClientFeedback)
        }
        ClientPlayConnectionEvents.DISCONNECT.register { _, client -> client.execute(Capturer::reset) }
    }
}
