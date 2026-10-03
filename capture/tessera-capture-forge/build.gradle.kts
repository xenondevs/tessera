plugins {
    id("tessera.kotlin-conventions")
    id("tessera.bundle-conventions")
    alias(libs.plugins.mod.dev)
}

val modId = project.name.replace('-', '_')

neoForge {
    version = libs.versions.neoforge.asProvider().get()
    accessTransformers.from("src/main/resources/META-INF/accesstransformer.cfg")
    
    runs {
        create("client") { client() }
        
        configureEach {
            systemProperty("forge.logging.markers", "REGISTRIES")
            logLevel = org.slf4j.event.Level.DEBUG
        }
    }
    
    mods {
        create(modId) {
            sourceSet(sourceSets.main.get())
            sourceSet(project(":capture:tessera-capture-common").the<SourceSetContainer>()["main"])
        }
    }
}

dependencies {
    bundle(project(":capture:tessera-capture-common"))
  
    jarJar(libs.kotlin.stdlib)
    jarJar(libs.adventure.api)
    jarJar(libs.adventure.key)
}

val generateModMetadata = tasks.register<ProcessResources>("generateModMetadata") {
    val mc = libs.versions.minecraft.get()
    val neo = libs.versions.neoforge.asProvider().get()
    val replaceProperties = mapOf(
        "minecraft_version" to mc,
        "minecraft_version_range" to listOf(mc),
        "neo_version" to neo,
        "mod_id" to modId,
        "mod_name" to "Tessera Capture",
        "mod_license" to "MIT",
        "mod_version" to project.version.toString(),
    )
    inputs.properties(replaceProperties)
    expand(replaceProperties)
    from("src/main/templates")
    into("build/generated/sources/modMetadata")
}

val generateModConstants = tasks.register<Copy>("generateModConstants") {
    val props = mapOf("mod_id" to modId)
    inputs.properties(props)
    expand(props)
    from("src/main/kotlin-templates")
    into(layout.buildDirectory.dir("generated/sources/modConstants"))
}

sourceSets.main {
    resources.srcDir(generateModMetadata)
    kotlin.srcDir(generateModConstants)
}
neoForge {
    ideSyncTask(generateModMetadata)
    ideSyncTask(generateModConstants)
}
