plugins {
    id("tessera.publish-conventions")
}

val dist = providers.gradleProperty("nativeDist").orNull?.let(::file)

publishing {
    publications {
        create<MavenPublication>("native") {
            artifactId = "tessera-native"
            pom.packaging = "pom"

            if (dist != null) {
                val libraries = dist.listFiles { file -> file.name.startsWith("tessera_ffi-") }.orEmpty().sortedBy { it.name }
                check(libraries.isNotEmpty()) { "no tessera_ffi-* files in $dist" }
                for (library in libraries) {
                    // tessera_ffi-linux-x86_64-v3.so -> classifier linux-x86_64-v3, extension so
                    val name = library.name.removePrefix("tessera_ffi-")
                    artifact(library) {
                        classifier = name.substringBeforeLast('.')
                        extension = name.substringAfterLast('.')
                    }
                }
                artifact(dist.resolve("manifest.json")) {
                    classifier = "manifest"
                    extension = "json"
                }
                artifact(dist.resolve("manifest.json.sigstore.json")) {
                    classifier = "manifest"
                    extension = "json.sigstore.json"
                }
            }
        }
    }
}
