plugins {
    id("tessera.kotlin-conventions")
    alias(libs.plugins.loom)
}


val bundle = configurations.dependencyScope("bundle")

val bundleClasspath = configurations.resolvable("bundleClasspath") {
    extendsFrom(bundle.get())
    isTransitive = false
}

val shade = configurations.dependencyScope("shade")

val shadeClasspath = configurations.resolvable("shadeClasspath") {
    extendsFrom(shade.get())
}

configurations.named("implementation") { extendsFrom(bundle.get()) }

dependencies {
    minecraft(libs.minecraft)
    
    implementation(libs.fabric.loader)
    implementation(libs.fabric.api)
    implementation(libs.fabric.kotlin)
    
    bundle(project(":capture:tessera-capture-common"))
    shade(libs.adventure.api)
}

tasks.jar {
    dependsOn(bundleClasspath)
    from(bundleClasspath.map { it.map(::zipTree) })
    from(shadeClasspath.map { it.map(::zipTree) })
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
