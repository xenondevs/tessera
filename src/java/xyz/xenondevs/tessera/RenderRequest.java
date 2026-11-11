package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

/**
 * A request to render in Tessera
 */
public sealed interface RenderRequest {
    
    /**
     * Create a new item render request with the given id, rendered at size x size
     */
    static Item item(String id, int size) {
        return new Item(id, size, Format.RGBA, Framing.GUI, FlatResolution.REQUESTED);
    }
    
    /**
     * Create a new model render request with the given id, rendered at size x size
     */
    static Model model(String id, int size) {
        return new Model(id, size, Format.RGBA, Framing.GUI);
    }
    
    /**
     * Create a new block state render request with the given id, rendered at size x size
     */
    static BlockState blockState(String id, int size) {
        return new BlockState(id, null, size, Format.RGBA, Framing.GUI);
    }
    
    /**
     * the namespaced id of the thing to be rendered
     */
    String id();
    
    /**
     * the width & height of the final output image
     */
    int size();
    
    /**
     * the {@link Format} of the output
     */
    Format format();
    
    /**
     * the {@link Framing} of the output
     */
    Framing framing();
    
    /**
     * Sets the output {@link Format} of the render
     */
    RenderRequest format(Format format);
    
    /**
     * Sets the output {@link Framing} of the render, only applies if the item is not 2d
     */
    RenderRequest framing(Framing framing);
    
    /**
     * Zoomed in/out and centered so geometry spans the image
     */
    RenderRequest fit();
    
    /**
     * Zoomed in/out and centered so geometry spans the image (minus margins)
     *
     * @param margin Fraction of the size left free on each side
     */
    RenderRequest fit(float margin);
    
    /**
     * An item {@link RenderRequest}
     *
     * @param id             the namespaced id of the item
     * @param size           the width & height of the final output image (unless the item is 2d and {@link flatResolution}
     * @param format         the {@link Format} of the output
     * @param framing        the {@link Framing} of the output
     * @param flatResolution if the image is 2d, leave the render at the largest layer's res
     */
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
        
        /**
         * Zoomed in/out and centered so geometry spans the image, only applies if the item is not 2d
         */
        @Override
        public Item fit() {
            return fit(0);
        }
        
        /**
         * Zoomed in/out and centered so geometry spans the image (minus margins), only applies if the item is not 2d
         *
         * @param margin Fraction of the size left free on each side
         */
        @Override
        public Item fit(float margin) {
            return framing(new Framing.Fit(margin));
        }
        
        /**
         * If the image is 2d, leave the render at the largest layer's res
         */
        public Item nativeResolution() {
            return new Item(id, size, format, framing, FlatResolution.NATIVE);
        }
        
    }
    
    /**
     * A model {@link RenderRequest}
     *
     * @param id      the namespaced id of the item
     * @param size    the width & height of the final output image
     * @param format  the {@link Format} of the output
     * @param framing the {@link Framing} of the output
     */
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
    
    /**
     * A block state {@link RenderRequest}
     *
     * @param id         the namespaced id of the item
     * @param properties the properties of the block state to be rendered
     * @param size       the width & height of the final output image
     * @param format     the {@link Format} of the output
     * @param framing    the {@link Framing} of the output
     */
    record BlockState(String id, @Nullable String properties, int size, Format format,
                      Framing framing) implements RenderRequest {
        
        /**
         * Sets the state properties of the state to be rendered
         */
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
    
    /**
     * Describes how the final render should be framed in the output
     */
    sealed interface Framing {
        
        Gui GUI = new Gui();
        
        /**
         * Inventory slot framing
         */
        record Gui() implements Framing {
        }
        
        /**
         * Zoomed in/out and centered until so geometry spans the image (minus margins)
         *
         * @param margin Fraction of the size left free on each side
         */
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
