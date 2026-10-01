package xyz.xenondevs.tessera;

import org.jetbrains.annotations.Nullable;

import java.lang.foreign.MemorySegment;
import java.util.concurrent.atomic.AtomicInteger;

import static xyz.xenondevs.tessera.TesseraNative.DESTROY_PACK;

public abstract sealed class ResourcePack implements AutoCloseable permits DirectoryPack, NioPack, ZipPack {
    
    private static final int OPEN = 0, OWNED = 1, CLOSED = 2;
    private final AtomicInteger state = new AtomicInteger(OPEN);
    
    abstract MemorySegment handle();
    
    void release() {
    }
    
    void markOwned() {
        if (!state.compareAndSet(OPEN, OWNED))
            throw new IllegalStateException("Resource pack is already closed or owned by another renderer");
    }
    
    void unmarkOwned() {
        state.compareAndSet(OWNED, OPEN);
    }
    
    @Nullable Throwable takeError() {
        return null;
    }
    
    @Override
    public final void close() {
        int prev = state.getAndSet(CLOSED);
        if (prev == CLOSED)
            return;
        if (prev == OPEN) {
            try {
                DESTROY_PACK.invokeExact(handle());
            } catch (Throwable ex) {
                throw new IllegalStateException("Destroying the native pack failed", ex);
            }
        }
        release();
    }
    
    
}
