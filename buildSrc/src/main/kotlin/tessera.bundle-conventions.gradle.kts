plugins {
    id("tessera.java-conventions")
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


tasks.jar {
    dependsOn(bundleClasspath)
    from(bundleClasspath.map { it.map(::zipTree) }) { exclude("module-info.class") }
    from(shadeClasspath.map { it.map(::zipTree) }) { exclude("module-info.class") }
}
