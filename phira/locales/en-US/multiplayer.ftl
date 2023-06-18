
multiplayer = Multiplayer

connect = Connectar
connect-must-login = Você deve entrar para usar o multiplayer
connect-success = Connectado com sucesso
connect-failed = Não foi possível conectar
connect-authorize-failed = A sua autorização falhou

create-room = Criar sala
create-room-success = Sala criada
create-room-failed = Não foi possível criar a sala

join-room = Entrar na sala
join-room-invalid-id = ID de sala invalido

leave-room = Sair da sala
leave-room-failed = Não foi possível sair da sala

disconnect = Desconectar

request-start = Iniciar o jogo
request-start-no-chart = Você não escolheu a fase.
request-start-failed = Não foi possível começar o jogo

ready = Pronto
ready-failed = Não foi possivel se preparar ;-;

cancel-ready = Cancelar

room-id = ID de sala: { $id }

download-failed = Failed to download chart

chat-placeholder = Say something…
chat-send = Send
chat-empty = Message is empty
chat-sent = Sent
chat-send-failaed = Failed to send message

select-chart-host-only = Only the host can select chart
select-chart-local = Can't select local chart
select-chart-failed = Failed to select chart

msg-create-room = `{ $user }` created the room
msg-join-room = `{ $user }` joined the room
msg-leave-room = `{ $user }` left the room
msg-new-host = `{ $user }` became the new host
msg-select-chart = The host `{ $user }` selected chart `{ $chart }` (#{ $id })
msg-game-start = The host `{ $user }` started the game
msg-ready = `{ $user }` is ready
msg-cancel-ready = `{ $user }` cancelled ready
msg-cancel-game = `{ $user }` cancelled the game
msg-start-playing = Game start
msg-played = `{ $user }` finished playing: { $score } ({ $accuracy }){ $full-combo ->
  [true] , full combo
  *[other] {""}
}
msg-game-end = Game ended
