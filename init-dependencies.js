const { execSync } = require("child_process");
const fs = require("fs");

// Load the dependencies.json file
const dependencies = JSON.parse(fs.readFileSync("dependencies.json", "utf8")).repositories;

// Clone or pull each repository
dependencies.forEach((repo) => {
    const targetDir = repo.target;

    // Check if the target directory already exists
    if (fs.existsSync(targetDir)) {
        console.log(`Pulling latest changes for ${repo.name}...`);
        execSync(`git -C ${targetDir} pull`, { stdio: "inherit" });
    } else {
        console.log(`Cloning ${repo.name}...`);
        execSync(`git clone -b ${repo.branch} ${repo.url} ${targetDir}`, { stdio: "inherit" });
    } 
});

console.log("All dependencies installed.");
