plugins {
    id("tessera.kotlin-conventions")
    id("tessera.bundle-conventions")
    alias(libs.plugins.loom)
}

dependencies {
    minecraft(libs.minecraft)
    
    implementation(libs.fabric.loader)
    implementation(libs.fabric.api)
    implementation(libs.fabric.kotlin)
    
    bundle(project(":capture:tessera-capture-common"))
    shade(libs.adventure.api)
}

tasks.processResources {
    val props = mapOf(
        "version" to project.version.toString(),
        "loader_version" to libs.versions.fabric.loader.get()
    )
    inputs.properties(props)
    filesMatching("fabric.mod.json") { expand(props) }
}

loom {
    accessWidenerPath = file("src/main/resources/tessera-capture.accesswidener")
}
