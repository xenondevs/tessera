package xyz.xenondevs.tessera.capture.util

import com.mojang.brigadier.Command
import com.mojang.brigadier.arguments.ArgumentType
import com.mojang.brigadier.builder.ArgumentBuilder
import com.mojang.brigadier.builder.LiteralArgumentBuilder
import com.mojang.brigadier.builder.RequiredArgumentBuilder
import com.mojang.brigadier.context.CommandContext
import net.kyori.adventure.text.Component
import net.kyori.adventure.text.TextComponent

interface CommandFeedback<S> {
    fun info(source: S, message: Component)
    fun error(source: S, message: Component)
}

fun <S> literal(name: String): LiteralArgumentBuilder<S> =
    LiteralArgumentBuilder.literal(name)

fun <S, T> argument(name: String, type: ArgumentType<T>): RequiredArgumentBuilder<S, T> =
    RequiredArgumentBuilder.argument(name, type)

fun <S, B : ArgumentBuilder<S, B>> B.runs(action: (CommandContext<S>) -> Unit): B =
    this.executes { action(it); Command.SINGLE_SUCCESS }

fun <S> CommandFeedback<S>.info(ctx: CommandContext<S>, message: TextComponent.Builder.() -> Unit) {
    info(ctx.source, Component.text().apply(message).build())
}

fun <S> CommandFeedback<S>.error(ctx: CommandContext<S>, message: TextComponent.Builder.() -> Unit) {
    error(ctx.source, Component.text().apply(message).build())
}


