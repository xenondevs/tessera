package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

public sealed interface RenderRequest {
    
    String id();
    
    Output output();
    
    record Item(String id, Output output) implements RenderRequest {
    }
    
    record Model(String id, Output output) implements RenderRequest {
    }
    
    record BlockState(String id, @Nullable String properties, Framing framing,
                      Output output) implements RenderRequest {
    }
    
    sealed interface Framing {
        record Gui() implements Framing {
        }
        
        record Fit(float margin) implements Framing {
        }
    }
    
    record Output(int size, Format format) {
    }
    
    enum Format {
        /**
         * Raw pixels data, successful result is {@link RenderResult.Pixels}.
         */
        RGBA,
        /**
         * An encoded PNG, successful result is {@link RenderResult.Png}.
         */
        PNG
    }
}
