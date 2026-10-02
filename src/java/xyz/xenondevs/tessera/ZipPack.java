package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

import java.io.File;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;

public final class ZipPack extends ResourcePack {
    
    final MemorySegment handle;
    
    public ZipPack(File zipFile, @Nullable String root) throws Throwable {
        try (var scratch = Arena.ofConfined()) {
            var pathPtr = scratch.allocateFrom(zipFile.getAbsolutePath());
            var rootPtr = root == null ? MemorySegment.NULL : scratch.allocateFrom(root);
            handle = (MemorySegment) TesseraNative.CREATE_ZIP_PACK.invokeExact(
                pathPtr,
                pathPtr.byteSize() - 1,  // appended \0
                rootPtr,
                root == null ? 0 : rootPtr.byteSize() - 1
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
