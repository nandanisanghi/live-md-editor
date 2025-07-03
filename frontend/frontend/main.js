const socket = new WebSocket("ws://localhost:3000/ws");

socket.onopen = () => {
  console.log("WebSocket connected.");
};

socket.onmessage = (event) => {
  document.getElementById("editor").value = event.data;
};

document.getElementById("editor").addEventListener("input", (e) => {
  socket.send(e.target.value);
});
