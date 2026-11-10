import org.gradle.jvm.tasks.Jar

plugins {
    id("tessera.kotlin-conventions")
}

version = providers.gradleProperty("captureVersion").get()

val libs = extensions.getByType<VersionCatalogsExtension>().named("libs")

tasks.named<Jar>("jar") {
    archiveClassifier = "MC-" + libs.findVersion("minecraft").get().requiredVersion
}
