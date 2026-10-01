package xyz.xenondevs.tessera;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

public class TesseraNative {
    
    static final Linker LINKER = Linker.nativeLinker();
    // TODO - library per arch(features) distribution, downloading and loading
    private static final SymbolLookup LIB = SymbolLookup.libraryLookup("tessera_ffi", Arena.global());
    
    static final MethodHandle LAST_ERROR = downcall("tessera_last_error", FunctionDescriptor.of(ADDRESS));
    
    static final MethodHandle CREATE_ZIP_PACK = downcall("tessera_create_zip_pack", FunctionDescriptor.of(ADDRESS, ADDRESS, JAVA_LONG, ADDRESS, JAVA_LONG));
    static final MethodHandle CREATE_DIRECTORY_PACK = downcall("tessera_create_directory_pack", FunctionDescriptor.of(ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle CREATE_JVM_PACK = downcall("tessera_create_jvm_pack", FunctionDescriptor.of(ADDRESS, ADDRESS, ADDRESS));
    static final MethodHandle RESERVE_SINK = downcall("tessera_reserve_sink", FunctionDescriptor.of(ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle PUSH_STRINGS = downcall("tessera_push_string", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle DESTROY_PACK = downcall("tessera_destroy_pack", FunctionDescriptor.ofVoid(ADDRESS));
    
    static final MethodHandle CREATE_RENDERER = downcall("tessera_create_renderer", FunctionDescriptor.of(ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle DESTROY_RENDERER = downcall("tessera_destroy_renderer", FunctionDescriptor.ofVoid(ADDRESS));
    
    static final MethodHandle RENDER = downcall("tessera_render", FunctionDescriptor.of(JAVA_INT, ADDRESS, ADDRESS, JAVA_LONG, ADDRESS));
    
    private static MethodHandle downcall(String name, FunctionDescriptor desc) {
        return LINKER.downcallHandle(LIB.find(name).orElseThrow(), desc);
    }
    
    static String lastError() {
        MemorySegment ptr;
        try {
            ptr = (MemorySegment) LAST_ERROR.invokeExact();
        } catch (Throwable ex) {
            throw new AssertionError(ex);
        }
        
        return ptr.equals(MemorySegment.NULL)
            ? "unknown native error"
            : ptr.reinterpret(Long.MAX_VALUE).getString(0);
    }
    
}
