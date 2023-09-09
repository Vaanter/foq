/**
* JetBrains Space Automation
* This Kotlin script file lets you automate build activities
* For more info, see https://www.jetbrains.com/help/space/automation.html
*/

job("Test Linux on latest container") {
    container(displayName = "Run script", image = "rust:latest") {
        shellScript {
            content = """
                set -e
                # Run tests with release optimizations
                cargo test --release --verbose
            """
        }
    }
}

job("Build Linux aarch64 with Cross") {
    host(displayName = "Cross") {
        requirements {
            workerTags("cross")
        }

        shellScript {
            content = """
                cross build --release --target aarch64-unknown-linux-gnu --verbose
            """
        }

        fileArtifacts {
            // Local path to artifact relative to working dir
            localPath = "target/aarch64-unknown-linux-gnu/release/foq"
            // Don't fail job if artifact is not found
            optional = true
            // Target path to artifact in file repository.
            remotePath = "foq-aarch64-unknown-linux-gnu"
            // Upload condition (job run result): SUCCESS (default), ERROR, ALWAYS
            onStatus = OnStatus.SUCCESS
        }
    }
}

job("Build Linux x86_64 with Cross") {
    host(displayName = "Cross") {
        requirements {
            workerTags("cross")
        }

        shellScript {
            content = """
                cross build --release --target x86_64-unknown-linux-gnu --verbose
            """
        }

        fileArtifacts {
            // Local path to artifact relative to working dir
            localPath = "target/x86_64-unknown-linux-gnu/release/foq"
            // Don't fail job if artifact is not found
            optional = true
            // Target path to artifact in file repository.
            remotePath = "foq-x86_64-unknown-linux-gnu"
            // Upload condition (job run result): SUCCESS (default), ERROR, ALWAYS
            onStatus = OnStatus.SUCCESS
        }
    }
}

job("Build Windows MSVC x86_64 with Cross") {
    host(displayName = "Cross") {
        requirements {
            workerTags("cross")
        }

        shellScript {
            content = """
                cross build --release --target x86_64-pc-windows-msvc --verbose
            """
        }

        fileArtifacts {
            // Local path to artifact relative to working dir
            localPath = "target/x86_64-pc-windows-msvc/release/foq.exe"
            // Don't fail job if artifact is not found
            optional = true
            // Target path to artifact in file repository.
            remotePath = "foq-x86_64-pc-windows-msvc.exe"
            // Upload condition (job run result): SUCCESS (default), ERROR, ALWAYS
            onStatus = OnStatus.SUCCESS
        }
    }
}
