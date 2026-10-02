package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.MemorySegment;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.atomic.AtomicReference;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;
import static xyz.xenondevs.tessera.TesseraNative.CREATE_JVM_PACK;
import static xyz.xenondevs.tessera.TesseraNative.LINKER;
import static xyz.xenondevs.tessera.TesseraNative.PUSH_STRINGS;
import static xyz.xenondevs.tessera.TesseraNative.RESERVE_SINK;

public final class NioPack extends ResourcePack {
    
    private static final MethodHandle READ, LIST;
    
    static {
        var lookup = MethodHandles.lookup();
        try {
            READ = lookup.findVirtual(NioPack.class, "read", MethodType.methodType(long.class, MemorySegment.class, long.class, MemorySegment.class));
            LIST = lookup.findVirtual(NioPack.class, "list", MethodType.methodType(int.class, MemorySegment.class));
        } catch (NoSuchMethodException | IllegalAccessException ex) {
            throw new ExceptionInInitializerError(ex);
        }
        
    }
    
    private final Path root;
    private final Arena arena = Arena.ofShared();
    private final AtomicReference<@Nullable Throwable> lastError = new AtomicReference<>();
    final MemorySegment handle;
    
    public NioPack(Path root) throws Throwable {
        if (root.getFileSystem().provider().getScheme().equals("jar")) {
            var ssp = root.toUri().getRawSchemeSpecificPart();
            if (URI.create(ssp.substring(0, ssp.indexOf("!/"))).getScheme().equals("file"))
                throw new IllegalArgumentException("Use ZipPack for on disk zip packs");
        }
        
        this.root = root;
        var read = LINKER.upcallStub(READ.bindTo(this), FunctionDescriptor.of(JAVA_LONG, ADDRESS, JAVA_LONG, ADDRESS), arena);
        var list = LINKER.upcallStub(LIST.bindTo(this), FunctionDescriptor.of(JAVA_INT, ADDRESS), arena);
        handle = (MemorySegment) CREATE_JVM_PACK.invokeExact(read, list);
        var ex = lastError.getAndSet(null);
        if (ex != null) {
            arena.close();
            throw ex;
        }
        if (handle.equals(MemorySegment.NULL)) {
            arena.close();
            throw new IllegalStateException(TesseraNative.lastError());
        }
    }
    
    @Override
    MemorySegment handle() {
        return handle;
    }
    
    @Override
    @Nullable Throwable takeError() {
        return lastError.getAndSet(null);
    }
    
    private long read(MemorySegment path, long pathLen, MemorySegment sink) {
        var rel = new String(path.reinterpret(pathLen).toArray(JAVA_BYTE), StandardCharsets.UTF_8);
        try (var ch = Files.newByteChannel(root.resolve(rel))) {
            var size = ch.size();
            var dst = ((MemorySegment) RESERVE_SINK.invokeExact(sink, size)).reinterpret(size).asByteBuffer();
            while (dst.hasRemaining()) {
                if (ch.read(dst) <= 0) break;
            }
            return dst.position();
        } catch (Throwable ex) {
            lastError.compareAndSet(null, ex);
            return -1;
        }
    }
    
    private int list(MemorySegment sink) {
        try (var walk = Files.walk(root); var scratch = Arena.ofConfined()) {
            var iter = walk.filter(Files::isRegularFile).iterator();
            while (iter.hasNext()) {
                var file = iter.next();
                var str = scratch.allocateFrom(root.relativize(file).toString());
                PUSH_STRINGS.invokeExact(sink, str, str.byteSize() - 1); // appended \0
            }
            return 0;
        } catch (Throwable ex) {
            lastError.compareAndSet(null, ex);
            return -1;
        }
    }
    
    @Override
    void release() {
        arena.close();
    }
    
}
