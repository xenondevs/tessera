package xyz.xenondevs.tessera;

import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

public sealed interface RenderRequest {
    
    @NotNull String id();
    
    int size();
    
    record Item(@NotNull String id, int size) implements RenderRequest {
    }
    
    record Model(@NotNull String id, int size) implements RenderRequest {
    }
    
    record BlockState(@NotNull String id, @Nullable String properties, int size,
                      @NotNull Framing framing) implements RenderRequest {
    }
    
    sealed interface Framing {
        record Gui() implements Framing {
        }
        
        record Fit(float margin) implements Framing {
        }
    }
    
}
