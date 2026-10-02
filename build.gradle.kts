plugins {
    id("tessera.java-conventions")
}

sourceSets {
    main {
        java.setSrcDirs(listOf("src/java"))
    }
}

dependencies {
    compileOnlyApi("org.jspecify:jspecify:1.0.0")
}
