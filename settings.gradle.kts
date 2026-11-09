rootProject.name = "tessera"

pluginManagement {
    repositories {
        maven("https://maven.fabricmc.net/") { name = "fabric" }
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        mavenCentral()
    }
    versionCatalogs {
        create("libs")
    }
}

if (!providers.gradleProperty("publishOnly").isPresent) {
    include("capture:tessera-capture-common")
    include("capture:tessera-capture-fabric")
    include("capture:tessera-capture-forge")
}

include("tessera-native")
project(":tessera-native").projectDir = file("native")
