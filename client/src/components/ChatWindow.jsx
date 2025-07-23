import React, { useState, useRef, useEffect } from 'react'
import { Card, Form, Button, InputGroup, Dropdown } from 'react-bootstrap'
import MessageList from './MessageList'

const ChatWindow = ({ user, activeGroup, messages, onSendMessage, onInviteUser, isConnected }) => {
  const [messageText, setMessageText] = useState('')
  const [isTyping, setIsTyping] = useState(false)
  const inputRef = useRef(null)

  useEffect(() => {
    // Focus input when group changes
    if (activeGroup && inputRef.current) {
      inputRef.current.focus()
    }
  }, [activeGroup])

  const handleSubmit = (e) => {
    e.preventDefault()
    if (messageText.trim() && isConnected && activeGroup) {
      onSendMessage(messageText)
      setMessageText('')
      setIsTyping(false)
    }
  }

  const handleInputChange = (e) => {
    setMessageText(e.target.value)
    setIsTyping(e.target.value.length > 0)
  }

  const handleKeyPress = (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSubmit(e)
    }
  }

  if (!activeGroup) {
    return (
      <div className="h-100 d-flex align-items-center justify-content-center">
        <div className="text-center text-muted">
          <i className="bi bi-chat-dots display-1 mb-3"></i>
          <h4>Welcome to Ruggine!</h4>
          <p>Select a group to start chatting or create a new one.</p>
        </div>
      </div>
    )
  }

  return (
    <div className="h-100 d-flex flex-column">
      {/* Chat Header */}
      <Card className="border-0 border-bottom rounded-0">
        <Card.Body className="py-3">
          <div className="d-flex align-items-center justify-content-between">
            <div className="d-flex align-items-center">
              <div className="me-3">
                <i className={`bi ${activeGroup.is_private ? 'bi-lock-fill' : 'bi-people-fill'} fs-4`}></i>
              </div>
              <div>
                <h5 className="mb-0">{activeGroup.name}</h5>
                {activeGroup.description && (
                  <small className="text-muted">{activeGroup.description}</small>
                )}
              </div>
            </div>
            
            <div className="d-flex align-items-center gap-2">
              <Button 
                variant="outline-primary" 
                size="sm"
                onClick={onInviteUser}
                title="Invita utenti al gruppo"
              >
                <i className="bi bi-person-plus-fill me-1"></i>
                Invita
              </Button>
              
              <Dropdown>
                <Dropdown.Toggle 
                  variant="outline-primary" 
                  size="sm" 
                  id="group-options"
                  className="border-0"
                >
                  <i className="bi bi-gear-fill"></i>
                </Dropdown.Toggle>
                <Dropdown.Menu>
                  <Dropdown.Item onClick={onInviteUser}>
                    <i className="bi bi-person-plus-fill me-2"></i>
                    Invita utenti
                  </Dropdown.Item>
                  <Dropdown.Divider />
                  <Dropdown.Item>
                    <i className="bi bi-info-circle-fill me-2"></i>
                    Info gruppo
                  </Dropdown.Item>
                </Dropdown.Menu>
              </Dropdown>
            </div>
          </div>
        </Card.Body>
      </Card>

      {/* Messages Area */}
      <div className="flex-grow-1 messages-container">
        <MessageList 
          messages={messages} 
          currentUser={user}
          groupId={activeGroup.id}
        />
      </div>

      {/* Message Input */}
      <Card className="border-0 border-top rounded-0 message-input">
        <Card.Body className="py-3">
          <Form onSubmit={handleSubmit}>
            <InputGroup>
              <Form.Control
                ref={inputRef}
                type="text"
                placeholder={
                  isConnected 
                    ? `Message ${activeGroup.name}...` 
                    : 'Connecting...'
                }
                value={messageText}
                onChange={handleInputChange}
                onKeyPress={handleKeyPress}
                disabled={!isConnected}
                style={{ borderRight: 'none' }}
              />
              <Button 
                variant="primary" 
                type="submit"
                disabled={!messageText.trim() || !isConnected}
              >
                <i className="bi bi-send-fill"></i>
              </Button>
            </InputGroup>
          </Form>
          {isTyping && (
            <small className="text-muted mt-1 d-block">
              Press Enter to send, Shift+Enter for new line
            </small>
          )}
        </Card.Body>
      </Card>
    </div>
  )
}

export default ChatWindow
