plugins {
    id("tessera.capture-conventions")
    alias(libs.plugins.loom)
}

dependencies {
    minecraft(libs.minecraft)
    
    compileOnly(libs.sponge.mixin)
    compileOnly(libs.mixinextras.common)
    
    api(libs.adventure.api)
}

loom {
    accessWidenerPath = file("../tessera-capture-fabric/src/main/resources/tessera-capture.accesswidener")
}
