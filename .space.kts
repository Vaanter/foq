/**
* JetBrains Space Automation
* This Kotlin script file lets you automate build activities
* For more info, see https://www.jetbrains.com/help/space/automation.html
*/

job("Build, latest") {
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