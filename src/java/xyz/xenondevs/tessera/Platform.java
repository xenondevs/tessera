package xyz.xenondevs.tessera;

import java.io.IOException;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Set;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_INT;

record Platform(Os os, Arch arch) {
    
    private static final Linker LINKER = Linker.nativeLinker();
    
    // release.yml builds the linux natives in manylinux_2_28 and fails if they need anything newer
    private static final int MIN_GLIBC_MAJOR = 2, MIN_GLIBC_MINOR = 28;
    
    // https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-isprocessorfeaturepresent
    private static final int PF_SSE4_2 = 38, PF_AVX2 = 40, PF_AVX512F = 41;
    
    // /proc/cpuinfo names for what each x86-64 psABI level adds
    private static final Set<String>
        X86_V2 = Set.of("ssse3", "sse4_1", "sse4_2", "popcnt", "cx16"),
        X86_V3 = Set.of("avx", "avx2", "bmi1", "bmi2", "f16c", "fma", "abm", "movbe", "xsave"),
        X86_V4 = Set.of("avx512f", "avx512bw", "avx512cd", "avx512dq", "avx512vl");
    
    static Platform current() {
        var name = System.getProperty("os.name").toLowerCase(Locale.ROOT);
        Os os;
        if (name.startsWith("windows")) {
            os = Os.WINDOWS;
        } else if (name.startsWith("mac") || name.startsWith("darwin")) {
            os = Os.OSX;
        } else if (name.startsWith("linux")) {
            os = Os.LINUX;
        } else throw new IllegalStateException("No native library for " + name);
        
        var arch = switch (System.getProperty("os.arch").toLowerCase(Locale.ROOT)) {
            case "amd64", "x86_64", "x64" -> Arch.X86_64;
            case "aarch64", "arm64" -> Arch.AARCH_64;
            default -> throw new IllegalStateException("No native library for " + System.getProperty("os.arch"));
        };
        return new Platform(os, arch);
    }
    
    List<String> variants() throws Throwable {
        var base = (os + "-" + arch).toLowerCase(Locale.ROOT);
        return switch (arch) {
            case X86_64 -> {
                var variants = new ArrayList<String>();
                for (int level = getX86Level(); level >= 2; --level)
                    variants.add(base + "-v" + level);
                yield variants;
            }
            case AARCH_64 -> os == Os.LINUX && getCpuInfo("Features").contains("sve")
                ? List.of(base + "-sve", base)
                : List.of(base);
        };
    }
    
    // x86-64 psABI level of this server or 2 when it can't be detected. Windows only reports SSE4.2, AVX2 and AVX-512F,
    // so v3 and v4 are inferred from those: every AVX2 CPU also has FMA, BMI1/2, F16C, LZCNT and MOVBE, and every AVX-512F
    // one also has BW, CD, DQ and VL.
    private int getX86Level() throws Throwable {
        return switch (os) {
            case LINUX -> {
                var flags = getCpuInfo("flags");
                if (!flags.isEmpty() && !flags.containsAll(X86_V2))
                    throw new IllegalStateException("An x86-64-v2 CPU (SSE4.2, POPCNT) is required");
                if (!flags.containsAll(X86_V3))
                    yield 2;
                yield flags.containsAll(X86_V4) ? 4 : 3;
            }
            case WINDOWS -> {
                // every windows the JDK runs on exports this
                var kernel32 = SymbolLookup.libraryLookup("kernel32", Arena.global());
                var symbol = kernel32.find("IsProcessorFeaturePresent").orElseThrow();
                var present = LINKER.downcallHandle(symbol, FunctionDescriptor.of(JAVA_INT, JAVA_INT));
                var avx2 = (int) present.invokeExact(PF_AVX2) != 0;
                if (avx2 && (int) present.invokeExact(PF_AVX512F) != 0)
                    yield 4;
                yield avx2 && (int) present.invokeExact(PF_SSE4_2) != 0 ? 3 : 2;
            }
            case OSX -> 2;
        };
    }
    
    // the first "<field> : a b c" line of /proc/cpuinfo
    private static Set<String> getCpuInfo(String field) throws IOException {
        var path = Path.of("/proc/cpuinfo");
        if (!Files.isReadable(path))
            return Set.of();
        
        try (var lines = Files.lines(path)) {
            return lines
                .map(line -> line.split(":", 2))
                .filter(parts -> parts.length == 2 && parts[0].trim().equals(field))
                .findFirst()
                .map(parts -> Set.copyOf(List.of(parts[1].trim().split("\\s+"))))
                .orElse(Set.of());
        }
    }
    
    static void requireGlibc() throws Throwable {
        var symbol = LINKER.defaultLookup().find("gnu_get_libc_version");
        if (symbol.isEmpty())
            throw new IllegalStateException("The linux natives need glibc, musl isn't supported");
        
        var handle = LINKER.downcallHandle(symbol.get(), FunctionDescriptor.of(ADDRESS));
        var version = ((MemorySegment) handle.invokeExact()).reinterpret(Long.MAX_VALUE).getString(0);
        // "2.39", sometimes with a suffix on the minor ("2.35-0ubuntu3")
        var parts = version.split("\\.");
        var major = Integer.parseInt(parts[0]);
        var minor = parts.length > 1 ? Integer.parseInt(parts[1].replaceAll("\\D.*", "")) : 0;
        if (major < MIN_GLIBC_MAJOR || major == MIN_GLIBC_MAJOR && minor < MIN_GLIBC_MINOR) {
            var min = MIN_GLIBC_MAJOR + "." + MIN_GLIBC_MINOR;
            throw new IllegalStateException("Glibc " + min + " or newer is required, found " + version);
        }
    }
    
    enum Os {
        WINDOWS, OSX, LINUX
    }
    
    enum Arch {
        X86_64, AARCH_64
    }
    
}
