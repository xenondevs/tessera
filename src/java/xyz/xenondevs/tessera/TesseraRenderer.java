package xyz.xenondevs.tessera;

import org.jetbrains.annotations.NotNull;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.StructLayout;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_FLOAT;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;
import static xyz.xenondevs.tessera.TesseraNative.CREATE_RENDERER;
import static xyz.xenondevs.tessera.TesseraNative.DESTROY_RENDERER;
import static xyz.xenondevs.tessera.TesseraNative.LINKER;
import static xyz.xenondevs.tessera.TesseraNative.RENDER;

public class TesseraRenderer implements AutoCloseable {
    
    private static final StructLayout REQUEST = MemoryLayout.structLayout(
        ADDRESS.withName("id"),
        JAVA_LONG.withName("idLen"),
        ADDRESS.withName("props"),
        JAVA_LONG.withName("propsLen"),
        JAVA_INT.withName("size"),
        JAVA_FLOAT.withName("margin"),
        JAVA_BYTE.withName("kind"),
        JAVA_BYTE.withName("framing"),
        MemoryLayout.paddingLayout(6)
    );
    
    private static final long
        ID = requestOffset("id"), ID_LEN = requestOffset("idLen"), PROPS = requestOffset("props"), PROPS_LEN = requestOffset("propsLen"),
        SIZE = requestOffset("size"), MARGIN = requestOffset("margin"), KIND = requestOffset("kind"), FRAMING = requestOffset("framing");
    private static final byte KIND_ITEM = 0, KIND_MODEL = 1, KIND_BLOCK_STATE = 2;
    private static final byte FRAMING_GUI = 0, FRAMING_FIT = 1;
    
    private static final FunctionDescriptor RESULT_DESCRIPTOR = FunctionDescriptor.of(JAVA_INT, JAVA_LONG, ADDRESS, JAVA_LONG, ADDRESS);
    private static final MethodHandle ON_RESULT;
    
    static {
        try {
            ON_RESULT = MethodHandles.lookup().findVirtual(
                Batch.class,
                "onResult",
                MethodType.methodType(
                    int.class, long.class, MemorySegment.class,
                    long.class, MemorySegment.class
                )
            );
        } catch (NoSuchMethodException | IllegalAccessException ex) {
            throw new ExceptionInInitializerError(ex);
        }
    }
    
    private final AtomicBoolean closed = new AtomicBoolean();
    private final List<ResourcePack> packs;
    private final MemorySegment handle;
    
    public TesseraRenderer(@NotNull List<@NotNull ResourcePack> packs) throws Throwable {
        if (packs.isEmpty())
            throw new IllegalArgumentException("At least one resource pack is required");
        
        this.packs = List.copyOf(packs);
        try (var scratch = Arena.ofConfined()) {
            var array = scratch.allocate(ADDRESS, this.packs.size());
            for (int i = 0; i < this.packs.size(); ++i) {
                array.setAtIndex(ADDRESS, i, this.packs.get(i).handle());
            }
            
            var marked = 0;
            try {
                for (var pack : this.packs) {
                    pack.markOwned();
                    ++marked;
                }
            } catch (IllegalStateException ex) {
                for (int i = 0; i < marked; ++i) {
                    this.packs.get(i).unmarkOwned();
                }
                throw ex;
            }
            handle = (MemorySegment) CREATE_RENDERER.invokeExact(array, (long) this.packs.size());
        }
        if (handle.equals(MemorySegment.NULL)) {
            var message = TesseraNative.lastError();
            rethrowPackErrors(false);
            closePacks();
            throw new IllegalStateException(message);
        }
        rethrowPackErrors(true);
    }
    
    public @NotNull List<@NotNull RenderResult> render(@NotNull List<? extends @NotNull RenderRequest> requests) throws Throwable {
        if (closed.get())
            throw new IllegalStateException("Renderer is closed");
        if (requests.isEmpty())
            return List.of();
        
        var batch = new Batch(requests.size());
        int status;
        try (var arena = Arena.ofConfined()) {
            var array = arena.allocate(REQUEST, requests.size());
            for (int i = 0; i < requests.size(); ++i) {
                write(array.asSlice(i * REQUEST.byteSize(), REQUEST), requests.get(i), arena);
            }
            var onResult = LINKER.upcallStub(ON_RESULT.bindTo(batch), RESULT_DESCRIPTOR, arena);
            status = (int) RENDER.invokeExact(handle, array, (long) requests.size(), onResult);
        }
        
        rethrowPackErrors(false);
        switch (status) {
            case 0 -> { }
            case 1 -> throw batch.failure;
            default -> throw new IllegalArgumentException(TesseraNative.lastError());
        }
        return List.of(batch.results);
    }
    
    private static void write(MemorySegment el, RenderRequest request, Arena arena) {
        var id = arena.allocateFrom(request.id());
        el.set(ADDRESS, ID, id);
        el.set(JAVA_LONG, ID_LEN, id.byteSize() - 1); // appended \0
        el.set(JAVA_INT, SIZE, request.size());
        switch (request) {
            case RenderRequest.Item _ -> el.set(JAVA_BYTE, KIND, KIND_ITEM);
            case RenderRequest.Model _ -> el.set(JAVA_BYTE, KIND, KIND_MODEL);
            case RenderRequest.BlockState(var _, var properties, var _, var framing) -> {
                if (properties != null) {
                    var props = arena.allocateFrom(properties);
                    el.set(ADDRESS, PROPS, props);
                    el.set(JAVA_LONG, PROPS_LEN, props.byteSize() - 1);
                }
                el.set(JAVA_BYTE, KIND, KIND_BLOCK_STATE);
                switch (framing) {
                    case RenderRequest.Framing.Gui() -> el.set(JAVA_BYTE, FRAMING, FRAMING_GUI);
                    case RenderRequest.Framing.Fit(var margin) -> {
                        el.set(JAVA_BYTE, FRAMING, FRAMING_FIT);
                        el.set(JAVA_FLOAT, MARGIN, margin);
                    }
                }
            }
        }
    }
    
    private void rethrowPackErrors(boolean closeOnFailure) throws Throwable {
        for (var pack : packs) {
            var ex = pack.takeError();
            if (ex != null) {
                if (closeOnFailure) {
                    try {
                        close();
                    } catch (Throwable closeEx) {
                        ex.addSuppressed(closeEx);
                    }
                }
                throw ex;
            }
        }
    }
    
    private static long requestOffset(String field) {
        return REQUEST.byteOffset(MemoryLayout.PathElement.groupElement(field));
    }
    
    @Override
    public void close() {
        if (!closed.compareAndSet(false, true))
            return;
        try {
            DESTROY_RENDERER.invokeExact(handle);
        } catch (Throwable ex) {
            throw new AssertionError("Destroying the native renderer failed", ex);
        }
        closePacks();
    }
    
    private void closePacks() {
        packs.forEach(ResourcePack::close);
    }
    
    private static final class Batch {
        
        final RenderResult[] results;
        Throwable failure;
        
        Batch(int count) {
            results = new RenderResult[count];
        }
        
        int onResult(long index, MemorySegment png, long pngLen, MemorySegment error) {
            try {
                results[(int) index] = error.equals(MemorySegment.NULL)
                    ? new RenderResult.Success(png.reinterpret(pngLen).toArray(JAVA_BYTE))
                    : new RenderResult.Failure(error.reinterpret(Long.MAX_VALUE).getString(0));
                return 0;
            } catch (Throwable ex) {
                // escaping an upcall would kill the JVM
                failure = ex;
                return 1;
            }
        }
        
    }
}
