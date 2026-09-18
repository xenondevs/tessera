package xyz.xenondevs.tessera.capture.fabric

import net.fabricmc.fabric.api.client.command.v2.FabricClientCommandSource
import net.kyori.adventure.text.Component
import net.kyori.adventure.text.TextComponent
import net.kyori.adventure.text.TranslatableComponent
import net.kyori.adventure.text.event.ClickEvent
import net.kyori.adventure.text.event.HoverEvent
import net.kyori.adventure.text.format.Style
import net.kyori.adventure.text.format.TextColor
import net.kyori.adventure.text.format.TextDecoration
import net.minecraft.network.chat.MutableComponent
import xyz.xenondevs.tessera.capture.util.CommandFeedback
import java.net.URI
import net.minecraft.network.chat.ClickEvent as MinecraftClickEvent
import net.minecraft.network.chat.Component as MinecraftComponent
import net.minecraft.network.chat.HoverEvent as MinecraftHoverEvent
import net.minecraft.network.chat.Style as MinecraftStyle
import net.minecraft.network.chat.TextColor as MinecraftTextColor

object FabricClientFeedback : CommandFeedback<FabricClientCommandSource> {
    override fun info(source: FabricClientCommandSource, message: Component) {
        source.sendFeedback(message.toVanilla())
    }
    
    override fun error(source: FabricClientCommandSource, message: Component) {
        source.sendError(message.toVanilla())
    }
}

fun Component.toVanilla(): MutableComponent {
    val base = when (this) {
        is TextComponent -> MinecraftComponent.literal(content())
        is TranslatableComponent -> MinecraftComponent.translatable(key())
        else -> throw IllegalArgumentException("Unsupported component type ${this::class.simpleName}")
    }
    
    children().forEach { base.append(it.toVanilla()) }
    return base.withStyle(style().toVanilla())
}

fun Style.toVanilla(): MinecraftStyle {
    fun hasDecoration(dec: TextDecoration): Boolean {
        return this.decoration(dec) == TextDecoration.State.TRUE
    }
    
    val isBold = hasDecoration(TextDecoration.BOLD)
    val isItalic = hasDecoration(TextDecoration.ITALIC)
    val isUnderlined = hasDecoration(TextDecoration.UNDERLINED)
    val isStrikethrough = hasDecoration(TextDecoration.STRIKETHROUGH)
    val isObfuscated = hasDecoration(TextDecoration.OBFUSCATED)
    
    return MinecraftStyle.EMPTY
        .withBold(isBold)
        .withItalic(isItalic)
        .withUnderlined(isUnderlined)
        .withStrikethrough(isStrikethrough)
        .withObfuscated(isObfuscated)
        .withColor(color()?.toVanilla())
        .withHoverEvent(hoverEvent()?.toVanilla())
        .withClickEvent(clickEvent()?.toVanilla())
}

fun TextColor.toVanilla(): MinecraftTextColor {
    return MinecraftTextColor.fromRgb(this.value())
}

fun <V : Any> HoverEvent<V>.toVanilla(): MinecraftHoverEvent? {
    return when (this.action()) {
        HoverEvent.Action.SHOW_TEXT -> MinecraftHoverEvent.ShowText((this.value() as Component).toVanilla())
        else -> null
    }
}

fun <T : ClickEvent.Payload> ClickEvent<T>.toVanilla(): MinecraftClickEvent? {
    return when (this.action()) {
        ClickEvent.Action.OPEN_URL -> MinecraftClickEvent.OpenUrl(URI((this.payload() as ClickEvent.Payload.Text).value()))
        ClickEvent.Action.OPEN_FILE -> MinecraftClickEvent.OpenFile((this.payload() as ClickEvent.Payload.Text).value())
        ClickEvent.Action.RUN_COMMAND -> MinecraftClickEvent.RunCommand((this.payload() as ClickEvent.Payload.Text).value())
        else -> null
    }
}
