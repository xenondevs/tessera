package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

public sealed interface RenderResult {
    
    record Png(byte[] png, @Nullable Bounds bounds) implements RenderResult {
    }
    
    record Pixels(byte[] rgba, int size, @Nullable Bounds bounds) implements RenderResult {
    }
    
    record Failure(String error) implements RenderResult {
    }
    
    record Bounds(int minX, int maxX, int minY, int maxY) {
    }
    
}
