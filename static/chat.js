window.onload = function() {
var connection = new WebSocket('ws://127.0.0.1:3012/ws');
var token = -1;
var logged_in = false;

function OutputMessages(messages) {
	var chatWindow = document.getElementById('chatwindow');
	chatWindow.innerHTML = '';
	for (i in messages) {
		var newMessage = document.createElement('p');
		newMessage.innerHTML = '<span class="uname">' + messages[i].uname + '</span> <span>' + decodeURI(messages[i].message) + '</span>';
		chatWindow.appendChild(newMessage);
	}
}

function ClearError() {
	document.getElementById("wserr").innerHTML = '';
}
function ProcessLogin(message_json) {
	if(message_json.status == 'success') {
		ClearError();
		document.getElementById('uname_input').hidden = true;
		document.getElementById('login_button').hidden = true;
		document.getElementById('message_input').hidden = false;
		document.getElementById('send_button').hidden = false;

		token = message_json.token;
		logged_in = true;

		window.setInterval(SendPing, 1000);

		OutputMessages(message_json.messages);
	} else {
		document.getElementById('login_button').disabled = false;
		document.getElementById('uname_input').disabled = false;
		document.getElementById("wserr").innerHTML = "Error: " + message_json.err;
	}
}

function ProcessMessage(message_json) {
	if(message_json.status == 'success') {
		ClearError();
		OutputMessages(message_json.messages);
	} else {
		document.getElementById("wserr").innerHTML = "Error: " + message_json.err;
	}
}
function ProcessPing(message_json) {
	if(message_json.status == 'success') {
		OutputMessages(message_json.messages);
	} else {
		document.getElementById("wserr").innerHTML = "Error: " + message_json.err;
	}
}

connection.onerror = function (error) {
	var errmes = document.getElementById("wserr");
	errmes.innerHTML = "WebSocket error: " + error;
}

connection.onmessage = function (e) {
	var message_json = JSON.parse(e.data);
	switch (message_json.type) {
		case "login": ProcessLogin(message_json); break;
		case "message": ProcessMessage(message_json); break;
		case "ping": ProcessPing(message_json); break;
		default: break;
	}
}

function OnLoginClick() {
	var uname = document.getElementById('uname_input').value;
	connection.send('{"type":"login","uname":"' + uname + '"}');
	document.getElementById('login_button').disabled = true;
	document.getElementById('uname_input').disabled = true;
}

function OnSendClick() {
	var content = document.getElementById('message_input').value;
	document.getElementById('message_input').value = '';
	content = encodeURI(content);
	connection.send('{"type":"message","token":' + token + ',"message":"' + content+'"}');
}

var loginButton = document.getElementById("login_button");
loginButton.addEventListener('click', OnLoginClick);

var sendButton = document.getElementById("send_button");
sendButton.addEventListener('click', OnSendClick);

function SendPing() {
	if(logged_in)
		connection.send('{"type":"ping","token":' + token + '}');
}

}
