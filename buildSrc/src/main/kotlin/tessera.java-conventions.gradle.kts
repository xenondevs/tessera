plugins {
    `java-library`
}

group = "xyz.xenondevs.tessera"
version = providers.gradleProperty("tesseraVersion").get()

val libs = extensions.getByType<VersionCatalogsExtension>().named("libs")
val jvmTarget = libs.findVersion("jvm").get().requiredVersion.toInt()

java {
    toolchain.languageVersion = JavaLanguageVersion.of(jvmTarget)
}

tasks.withType<JavaCompile>().configureEach {
    options.encoding = "UTF-8"
    options.release.set(jvmTarget)
}

tasks.withType<AbstractArchiveTask>().configureEach {
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true
}
