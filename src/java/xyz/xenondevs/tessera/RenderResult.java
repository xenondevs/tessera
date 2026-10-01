package xyz.xenondevs.tessera;

import org.jetbrains.annotations.NotNull;

public sealed interface RenderResult {
    
    record Success(byte @NotNull [] png) implements RenderResult {
    }
    
    record Failure(@NotNull String error) implements RenderResult {
    }
    
}
