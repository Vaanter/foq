/**
* JetBrains Space Automation
* This Kotlin script file lets you automate build activities
* For more info, see https://www.jetbrains.com/help/space/automation.html
*/

job("Build Linux on latest container") {
    container(displayName = "Run script", image = "rust:latest") {
        shellScript {
            content = """
                set -e
                # Build the Rust project
                cargo build --release --verbose
                cargo test --release --verbose
            """
        }

        fileArtifacts {
            val artifactName = "foq"
            // Local path to artifact relative to working dir
            localPath = "target/release/$artifactName"
            // Don't fail job if artifact is not found
            optional = true
            // Target path to artifact in file repository.
            remotePath = "{{ run:number }}/$artifactName"
            // Upload condition (job run result): SUCCESS (default), ERROR, ALWAYS
            onStatus = OnStatus.SUCCESS
        }
    }
}

job("Build Windows on host") {
    host(displayName = "Windows") {
        requirements {
            os {
                type = OSType.Windows
            }
        }

        shellScript {
            content = """
                cargo build --release --verbose
                cargo test --release --verbose
            """
        }

        fileArtifacts {
            val artifactName = "foq.exe"
            // Local path to artifact relative to working dir
            localPath = "target/release/$artifactName"
            // Don't fail job if artifact is not found
            optional = true
            // Target path to artifact in file repository.
            remotePath = "{{ run:number }}/$artifactName"
            // Upload condition (job run result): SUCCESS (default), ERROR, ALWAYS
            onStatus = OnStatus.SUCCESS
        }
    }
}
