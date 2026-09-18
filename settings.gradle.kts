rootProject.name = "tessera"

pluginManagement {
    repositories {
        maven("https://maven.fabricmc.net/") { name = "fabric" }
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    versionCatalogs {
        create("libs")
    }
}

include("capture:tessera-capture-common")
include("capture:tessera-capture-fabric")
