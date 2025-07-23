import React, { useState, useEffect } from 'react'
import { Container, Row, Col } from 'react-bootstrap'
import LoginForm from './components/LoginForm'
import ChatInterface from './components/ChatInterface'
import { useWebSocket } from './hooks/useWebSocket'

function App() {
  const [user, setUser] = useState(null)
  const [token, setToken] = useState(localStorage.getItem('ruggine_token'))
  const { 
    socket, 
    isConnected, 
    sendMessage, 
    messages, 
    groups, 
    users,
    pendingInvites,
    connect,
    disconnect 
  } = useWebSocket()

  useEffect(() => {
    // Connetti sempre all'avvio dell'app se non c'è una connessione
    if (!isConnected && !socket) {
      connect()
    }
  }, []) // Solo all'avvio

  const handleLogin = (userData, authToken) => {
    setUser(userData)
    setToken(authToken)
    localStorage.setItem('ruggine_token', authToken)
    localStorage.setItem('ruggine_user', JSON.stringify(userData))
    // No need to reconnect - login was done on the current connection
  }

  const handleLogout = () => {
    if (socket) {
      sendMessage({
        type: 'Logout'
      })
    }
    setUser(null)
    setToken(null)
    localStorage.removeItem('ruggine_token')
    localStorage.removeItem('ruggine_user')
    disconnect()
  }

  if (!user) {
    return (
      <LoginForm 
        onLogin={handleLogin}
        onSendMessage={sendMessage}
        isConnected={isConnected}
      />
    )
  }

  return (
    <Container fluid className="chat-container">
      <ChatInterface
        user={user}
        isConnected={isConnected}
        messages={messages}
        groups={groups}
        users={users}
        pendingInvites={pendingInvites}
        onSendMessage={sendMessage}
        onLogout={handleLogout}
      />
    </Container>
  )
}

export default App
