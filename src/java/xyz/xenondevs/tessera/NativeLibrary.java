package xyz.xenondevs.tessera;

import org.jspecify.annotations.Nullable;

import java.io.IOException;
import java.lang.foreign.Arena;
import java.lang.foreign.SymbolLookup;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.FileSystemException;
import java.nio.file.Files;
import java.nio.file.NoSuchFileException;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.time.Duration;
import java.util.HexFormat;
import java.util.List;
import java.util.Locale;
import java.util.Properties;
import java.util.Set;

import static com.sun.nio.file.ExtendedOpenOption.NOSHARE_DELETE;
import static com.sun.nio.file.ExtendedOpenOption.NOSHARE_WRITE;
import static java.nio.file.StandardCopyOption.ATOMIC_MOVE;
import static java.nio.file.StandardCopyOption.REPLACE_EXISTING;
import static java.nio.file.StandardOpenOption.READ;
import static java.nio.file.attribute.PosixFilePermissions.asFileAttribute;
import static java.nio.file.attribute.PosixFilePermissions.fromString;

final class NativeLibrary {
    
    private static final String DEFAULT_REPOSITORY = "https://repo.xenondevs.xyz/releases/";
    private static final String ARTIFACT_PATH = "xyz/xenondevs/tessera/tessera-native/";
    
    private static final Set<OpenOption> EXCLUSIVE = Set.of(READ, NOSHARE_WRITE, NOSHARE_DELETE);
    
    static SymbolLookup load() throws Throwable {
        var explicit = getSetting("path");
        if (explicit != null)
            return SymbolLookup.libraryLookup(Path.of(explicit), Arena.global());
        
        var platform = Platform.current();
        if (platform.os() == Platform.Os.LINUX)
            Platform.requireGlibc();
        
        var manifest = new Properties();
        try (var in = NativeLibrary.class.getResourceAsStream("natives.properties")) {
            if (in == null)
                throw new IllegalStateException("This build embeds no natives, set tessera.native.path to a local one");
            manifest.load(in);
        }
        var version = manifest.getProperty("version");
        
        var forced = getSetting("variant");
        var candidates = forced != null ? List.of(forced) : platform.variants();
        var variant = candidates.stream()
            .filter(candidate -> manifest.containsKey(candidate + ".sha256"))
            .findFirst()
            .orElseThrow(() -> new IllegalStateException("Tessera " + version + " has no native library for " + candidates));
        var file = manifest.getProperty(variant + ".file");
        var size = Long.parseLong(manifest.getProperty(variant + ".size"));
        var hash = manifest.getProperty(variant + ".sha256");
        
        var cache = getCacheDir(platform.os()).resolve(version);
        if (cache.getFileSystem().supportedFileAttributeViews().contains("posix"))
            Files.createDirectories(cache, asFileAttribute(fromString("rwx------")));
        else Files.createDirectories(cache);
        
        var target = cache.resolve(file);
        var library = loadVerified(target, size, hash, platform.os());
        if (library != null)
            return library;
        
        var repository = getSetting("repository");
        var extension = file.substring(file.lastIndexOf('.') + 1);
        var url = URI.create(repository != null ? repository : DEFAULT_REPOSITORY)
            .resolve(ARTIFACT_PATH + version + "/tessera-native-" + version + "-" + variant + "." + extension);
        var download = Files.createTempFile(cache, file, ".part");
        try {
            fetch(url, download);
            try (var channel = FileChannel.open(download)) {
                if (!matches(channel, size, hash))
                    throw new IOException(url + " doesn't match the hash tessera " + version + " was built with");
            }
            
            FileSystemException blocked = null;
            try {
                Files.move(download, target, REPLACE_EXISTING, ATOMIC_MOVE);
            } catch (FileSystemException ex) {
                // windows can't replace a library another JVM is verifying or has loaded, which is fine if it's the right file
                blocked = ex;
            }
            library = loadVerified(target, size, hash, platform.os());
            if (library == null)
                throw blocked != null ? blocked : new IOException(target + " was replaced while loading it");
            return library;
        } finally {
            Files.deleteIfExists(download);
        }
    }
    
    private static @Nullable SymbolLookup loadVerified(Path file, long size, String hash, Platform.Os os) throws IOException {
        try (var channel = FileChannel.open(file, os == Platform.Os.WINDOWS ? EXCLUSIVE : Set.of(READ))) {
            return matches(channel, size, hash) ? SymbolLookup.libraryLookup(file, Arena.global()) : null;
        } catch (NoSuchFileException ex) {
            return null;
        }
    }
    
    private static @Nullable String getSetting(String name) {
        var value = System.getProperty("tessera.native." + name);
        return value != null ? value : System.getenv("TESSERA_NATIVE_" + name.toUpperCase(Locale.ROOT));
    }
    
    private static Path getCacheDir(Platform.Os os) {
        var explicit = getSetting("cache");
        if (explicit != null)
            return Path.of(explicit);
        
        var home = Path.of(System.getProperty("user.home"));
        var base = switch (os) {
            case WINDOWS -> {
                var local = System.getenv("LOCALAPPDATA");
                yield local != null ? Path.of(local) : home.resolve("AppData/Local");
            }
            case OSX -> home.resolve("Library/Caches");
            case LINUX -> {
                var xdg = System.getenv("XDG_CACHE_HOME");
                yield xdg != null ? Path.of(xdg) : home.resolve(".cache");
            }
        };
        return base.resolve("tessera/natives");
    }
    
    private static void fetch(URI url, Path to) throws IOException, InterruptedException {
        if (url.getScheme().equals("file")) {
            Files.copy(Path.of(url), to, REPLACE_EXISTING);
            return;
        }
        
        var builder = HttpClient.newBuilder().followRedirects(HttpClient.Redirect.NORMAL).connectTimeout(Duration.ofSeconds(10));
        try (var client = builder.build()) {
            var request = HttpRequest.newBuilder(url).timeout(Duration.ofMinutes(2)).build();
            var response = client.send(request, HttpResponse.BodyHandlers.ofFile(to));
            if (response.statusCode() != 200)
                throw new IOException("Downloading " + url + " failed with HTTP " + response.statusCode());
        }
    }
    
    private static boolean matches(FileChannel channel, long size, String sha256) throws IOException {
        if (channel.size() != size)
            return false;
        
        var bytes = ByteBuffer.allocate((int) size);
        while (bytes.hasRemaining()) {
            if (channel.read(bytes) == -1)
                return false;
        }
        try {
            var digest = MessageDigest.getInstance("SHA-256").digest(bytes.array());
            return HexFormat.of().formatHex(digest).equals(sha256);
        } catch (NoSuchAlgorithmException ex) {
            throw new AssertionError(ex);
        }
    }
    
}
