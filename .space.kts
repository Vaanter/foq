/**
* JetBrains Space Automation
* This Kotlin script file lets you automate build activities
* For more info, see https://www.jetbrains.com/help/space/automation.html
*/

job("Build, run tests, and publish") {
    container(displayName = "Run script", image = "rustlang/rust:nightly") {
        shellScript {
            content = """
                set -e
                # Build the Rust project
                cargo build --verbose
            """
        }
    }
}