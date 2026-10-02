package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

public sealed interface RenderRequest {
    
    String id();
    
    int size();
    
    record Item(String id, int size) implements RenderRequest {
    }
    
    record Model(String id, int size) implements RenderRequest {
    }
    
    record BlockState(String id, @Nullable String properties, int size,
                      Framing framing) implements RenderRequest {
    }
    
    sealed interface Framing {
        record Gui() implements Framing {
        }
        
        record Fit(float margin) implements Framing {
        }
    }
    
}
