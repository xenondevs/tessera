plugins {
    id("tessera.java-conventions")
}

sourceSets {
    main {
        java.setSrcDirs(listOf("src/java"))
    }
}

dependencies {
    compileOnly("org.jetbrains:annotations:26.1.0")
}
