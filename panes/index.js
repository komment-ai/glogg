const fs = require("node:fs");
const { spawn } = require("node-pty");
const blessed = require("blessed");

function loadConfig(filePath) {
  const fileContent = fs.readFileSync(filePath, "utf8");
  return JSON.parse(fileContent);
}

// Create a blessed screen object
const screen = blessed.screen({
  smartCSR: true,
  title: "Multi-pane Tail Viewer",
});

// Define the number of panes and pane titles
const panes = loadConfig("./config.json");

const paneColor = (index) =>
  ["white", "green", "red", "orange", "blue"][index % 5];

// Function to create a pane
const createPane = (label, x, y, width, height, index) => {
  const box = blessed.box({
    label: label,
    top: y,
    left: x,
    width: width,
    height: height,
    border: { type: "line" },
    style: {
      border: { fg: "cyan" },
      label: {
        fg: paneColor(index),
        bold: true,
      },
    },
    scrollable: true,
    alwaysScroll: true,
    scrollbar: {
      ch: " ",
      inverse: true,
      style: {
        bg: "yellow",
      },
    },
    alwaysScroll: true,
    scrollable: true,
    mouse: true,
  });

  screen.append(box);
  return box;
};

// Layout parameters for 4 panes
const paneLayout = [
  { x: "0%", y: "0%", width: "50%", height: "50%" },
  { x: "50%", y: "0%", width: "50%", height: "50%" },
  { x: "0%", y: "50%", width: "50%", height: "50%" },
  { x: "50%", y: "50%", width: "50%", height: "50%" },
];

// Function to start a tail process in a pane
const startTailProcess = (box, command, args) => {
  const ptyProcess = spawn(command, args, {
    name: "xterm-color",
    cols: 80,
    rows: 24,
    env: process.env,
  });

  ptyProcess.onData((data) => {
    box.pushLine(data.trim());
    box.setScrollPerc(100);
    screen.render();
  });

  return ptyProcess;
};

// Create the panes and start tail processes
panes.forEach(({ filter, title = "all" }, index) => {
  const layout = paneLayout[index];
  const pane = createPane(
    title,
    layout.x,
    layout.y,
    layout.width,
    layout.height,
    index,
  );
  if (filter) {
    startTailProcess(pane, "../target/release/gtail", ["--filter", filter]);
  } else {
    pane.pushLine("No filter specified.");
  }
});

// Quit on Escape, Ctrl+C, or 'q'
screen.key(["escape", "C-c", "q"], () => process.exit(0));

// Render the screen
screen.render();
