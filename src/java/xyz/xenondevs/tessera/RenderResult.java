package xyz.xenondevs.tessera;

public sealed interface RenderResult {
    
    record Success(byte[] png) implements RenderResult {
    }
    
    record Failure(String error) implements RenderResult {
    }
    
}
