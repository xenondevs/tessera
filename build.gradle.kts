import groovy.json.JsonSlurper

plugins {
    id("tessera.java-conventions")
    id("tessera.publish-conventions")
}

// embed the hashes of the natives in the jar
val nativesManifest = tasks.register("nativesManifest") {
    val manifest = providers.gradleProperty("nativeDist").map { layout.projectDirectory.dir(it).file("manifest.json") }
    val output = layout.buildDirectory.dir("generated/natives")
    inputs.property("nativeDist", providers.gradleProperty("nativeDist").orElse(""))
    inputs.files(manifest.map { listOf(it) }.orElse(listOf())).withPropertyName("manifest")
    outputs.dir(output)
    doLast {
        val dir = output.get().asFile
        
        dir.deleteRecursively()
        if (!manifest.isPresent)
            return@doLast

        @Suppress("UNCHECKED_CAST")
        val json = JsonSlurper().parse(manifest.get().asFile) as Map<String, Any>
        @Suppress("UNCHECKED_CAST")
        val variants = (json["variants"] as Map<String, Map<String, Any>>).toSortedMap()
        //Properties.store adds a timestamp, which would make the jar irreproducible
        val text = buildString {
            append("version=").append(json["version"]).append('\n')
            for ((variant, entry) in variants)
                for (key in listOf("file", "size", "sha256"))
                    append(variant).append('.').append(key).append('=').append(entry[key]).append('\n')
        }
        dir.resolve("xyz/xenondevs/tessera").apply { mkdirs() }.resolve("natives.properties").writeText(text)
    }
}

sourceSets {
    main {
        java.setSrcDirs(listOf("src/java"))
        resources.srcDir(nativesManifest)
    }
}

java {
    withSourcesJar()
}

dependencies {
    compileOnlyApi("org.jspecify:jspecify:1.0.0")
    implementation("com.google.jimfs:jimfs:1.3.0")
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            artifactId = "tessera"
            from(components["java"])
        }
    }
}

tasks.named("publishMavenPublicationToXenondevsRepository") {
    dependsOn(":tessera-native:publishNativePublicationToXenondevsRepository")
}
