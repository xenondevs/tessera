plugins {
    `maven-publish`
}

group = "xyz.xenondevs.tessera"
version = providers.gradleProperty("tesseraVersion").get()

publishing {
    repositories {
        maven("https://repo.xenondevs.xyz/releases/") {
            name = "xenondevs"
            credentials(PasswordCredentials::class)
        }
    }
    
    publications.withType<MavenPublication>().configureEach {
        pom {
            name = project.name
            url = "https://github.com/xenondevs/tessera"
            licenses {
                license {
                    name = "MIT License"
                    url = "https://opensource.org/license/mit"
                }
            }
            scm {
                url = "https://github.com/xenondevs/tessera"
            }
        }
    }
}
