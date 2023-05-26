import os
import shutil
import subprocess

# setup environmental variables
env = os.environ.copy()
env["RUSTFLAGS"] = "-Cinstrument-coverage"
env["LLVM_PROFILE_FILE"] = "tests-%p-%m.profraw"

# clean the project
# retcode = subprocess.call(["cargo", "clean"], env=env)
#
# if retcode != 0:
#     print("Failed to clean the project! Exiting!")
#     exit(1)

try:
    # remove coverage folder
    shutil.rmtree("coverage")
except:
    pass

# build and run the tests
retcode = subprocess.call(["cargo", "test"], env=env)

if retcode != 0:
    print("Tests failed!")

# create coverage data
# THIS, FOR SOME REASON, DOES NOT EXCLUDE STUFF
# retcode = subprocess.call(
#     ["grcov", ".", "--binary-path", "./target/debug/", "-s", ".", "-t", "html", "--excl-line",
#      r'"#\[derive\("',
#      "--excl-start", r'"mod tests \{"', "--ignore", "*lab.rs", "--ignore", "*main.rs", "--ignore-not-existing", "-o",
#      "./coverage/"])
#
# if retcode != 0:
#     print("Failed to generate coverage data! Exiting!")
#     exit(1)

os.system(
    r'grcov . --binary-path ./target/debug/ -s . -t html --excl-line "(#\[derive\()|(^ *\.await;?)|'
    r'(#\[tracing::instrument\()|( *trace!\([^;]*\);)|( *debug!\([^;]*\);)|( *info!\([^;]*\);)|( *warn!\([^;]*\);)|'
    r'( *error!\([^;]*\);)" --excl-start "mod tests" --ignore-not-existing --ignore *main.rs --ignore *runner.rs'
    r' -o ./coverage/')

# cleanup residual files
for file in os.listdir():
    if file.endswith(".profraw"):
        print(f"Deleting: {file}")
        os.remove(file)
