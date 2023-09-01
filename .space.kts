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
                cargo build --verbose
            """
        }
    }
}

job("Build Windows on host") {
    host(displayName = "Windows") {
        requirements {
            os {
                type = OSType.Windows
                arch = "x86_64"
            }
        }

        shellScript {
            content = """
                cargo build --verbose
            """
        }
    }
}
