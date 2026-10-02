package xyz.xenondevs.tessera;

import java.io.File;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;

public final class DirectoryPack extends ResourcePack {
    
    final MemorySegment handle;
    
    public DirectoryPack(File dir) throws Throwable {
        try (var scratch = Arena.ofConfined()) {
            var pathPtr = scratch.allocateFrom(dir.getAbsolutePath());
            handle = (MemorySegment) TesseraNative.CREATE_DIRECTORY_PACK.invokeExact(
                pathPtr, pathPtr.byteSize() - 1  // appended \0
            );
        }
        if (handle.equals(MemorySegment.NULL)) {
            throw new IllegalStateException(TesseraNative.lastError());
        }
    }
    
    @Override
    MemorySegment handle() {
        return handle;
    }
    
}
