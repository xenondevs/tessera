package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

public sealed interface RenderResult {
    
    record Png(byte[] png, @Nullable Bounds bounds) implements RenderResult {
    }
    
    record Pixels(byte[] rgba, int size, @Nullable Bounds bounds) implements RenderResult {
    }
    
    /**
     * Nothing to draw, e.g. air
     */
    record Empty() implements RenderResult {
    }

    record Failure(Kind kind, String error) implements RenderResult {

        public enum Kind {
            INVALID_REQUEST,
            NO_VARIANT,
            NOT_PRIMED
        }

    }
    
    record Bounds(int minX, int maxX, int minY, int maxY) {
    }
    
}
