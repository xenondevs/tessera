package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

public sealed interface RenderRequest {
    
    static Item item(String id, int size) {
        return new Item(id, size, Format.RGBA, Framing.GUI, FlatResolution.REQUESTED);
    }
    
    static Model model(String id, int size) {
        return new Model(id, size, Format.RGBA, Framing.GUI);
    }
    
    static BlockState blockState(String id, int size) {
        return new BlockState(id, null, size, Format.RGBA, Framing.GUI);
    }
    
    String id();
    
    int size();
    
    Format format();
    
    Framing framing();
    
    RenderRequest format(Format format);
    
    RenderRequest framing(Framing framing);
    
    RenderRequest fit();
    
    RenderRequest fit(float margin);
    
    record Item(String id, int size, Format format, Framing framing,
                FlatResolution flatResolution) implements RenderRequest {
        
        @Override
        public Item format(Format format) {
            return new Item(id, size, format, framing, flatResolution);
        }
        
        @Override
        public Item framing(Framing framing) {
            return new Item(id, size, format, framing, flatResolution);
        }
        
        @Override
        public Item fit() {
            return fit(0);
        }
        
        @Override
        public Item fit(float margin) {
            return framing(new Framing.Fit(margin));
        }
        
        public Item nativeResolution() {
            return new Item(id, size, format, framing, FlatResolution.NATIVE);
        }
        
    }
    
    record Model(String id, int size, Format format, Framing framing) implements RenderRequest {
        
        @Override
        public Model format(Format format) {
            return new Model(id, size, format, framing);
        }
        
        @Override
        public Model framing(Framing framing) {
            return new Model(id, size, format, framing);
        }
        
        @Override
        public Model fit() {
            return fit(0);
        }
        
        @Override
        public Model fit(float margin) {
            return framing(new Framing.Fit(margin));
        }
        
    }
    
    record BlockState(String id, @Nullable String properties, int size, Format format,
                      Framing framing) implements RenderRequest {
        
        public BlockState properties(@Nullable String properties) {
            return new BlockState(id, properties, size, format, framing);
        }
        
        @Override
        public BlockState format(Format format) {
            return new BlockState(id, properties, size, format, framing);
        }
        
        @Override
        public BlockState framing(Framing framing) {
            return new BlockState(id, properties, size, format, framing);
        }
        
        @Override
        public BlockState fit() {
            return fit(0);
        }
        
        @Override
        public BlockState fit(float margin) {
            return framing(new Framing.Fit(margin));
        }
        
    }
    
    
    sealed interface Framing {
        
        Gui GUI = new Gui();
        
        record Gui() implements Framing {
        }
        
        record Fit(float margin) implements Framing {
            
            public Fit {
                // also rejects NaN
                if (!(margin < 0.5f))
                    throw new IllegalArgumentException("Margin must be below 0.5, was " + margin);
            }
            
        }
        
    }
    
    /**
     * Output res of 2d texture items
     */
    enum FlatResolution {
        /**
         * Scaled to requested size
         */
        REQUESTED,
        /**
         * Left at res of largest layer
         */
        NATIVE
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
