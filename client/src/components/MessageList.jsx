import React, { useEffect, useRef } from 'react'
import { Card } from 'react-bootstrap'
import dayjs from 'dayjs'

const MessageList = ({ messages, currentUser, groupId }) => {
  const messagesEndRef = useRef(null)

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }

  useEffect(() => {
    scrollToBottom()
  }, [messages])

  const getUserInitials = (username) => {
    return username?.substring(0, 2).toUpperCase() || '??'
  }

  const isOwnMessage = (message) => {
    return message.sender_id === currentUser.id
  }

  const formatMessageTime = (timestamp) => {
    const messageTime = dayjs(timestamp)
    const now = dayjs()
    
    if (messageTime.isSame(now, 'day')) {
      return messageTime.format('HH:mm')
    } else if (messageTime.isSame(now.subtract(1, 'day'), 'day')) {
      return `Yesterday ${messageTime.format('HH:mm')}`
    } else {
      return messageTime.format('MMM D, HH:mm')
    }
  }

  if (!messages || messages.length === 0) {
    return (
      <div className="h-100 d-flex align-items-center justify-content-center">
        <div className="text-center text-muted">
          <i className="bi bi-chat-text display-4 mb-3"></i>
          <p>No messages yet. Start the conversation!</p>
        </div>
      </div>
    )
  }

  return (
    <div className="h-100">
      {messages.map((message, index) => {
        const isOwn = isOwnMessage(message)
        const showAvatar = index === 0 || messages[index - 1].sender_id !== message.sender_id
        
        return (
          <div
            key={message.id}
            className={`d-flex mb-3 ${isOwn ? 'justify-content-end' : 'justify-content-start'}`}
          >
            {!isOwn && showAvatar && (
              <div className="user-avatar me-2 flex-shrink-0">
                {getUserInitials(message.sender_username || 'User')}
              </div>
            )}
            {!isOwn && !showAvatar && (
              <div style={{ width: '48px' }} className="flex-shrink-0"></div>
            )}
            
            <div className={`message-bubble ${isOwn ? 'own' : 'other'}`}>
              <Card className={`border-0 ${isOwn ? 'bg-primary text-white' : 'bg-light'}`}>
                <Card.Body className="py-2 px-3">
                  {!isOwn && showAvatar && (
                    <div className="fw-bold mb-1" style={{ fontSize: '0.85rem' }}>
                      {message.sender_username || 'Unknown User'}
                    </div>
                  )}
                  <div className="message-content mb-1">
                    {message.content}
                  </div>
                  <div 
                    className={`text-end ${isOwn ? 'text-white-50' : 'text-muted'}`}
                    style={{ fontSize: '0.75rem' }}
                  >
                    {formatMessageTime(message.sent_at)}
                    {message.edited_at && (
                      <span className="ms-1">(edited)</span>
                    )}
                  </div>
                </Card.Body>
              </Card>
            </div>
            
            {isOwn && showAvatar && (
              <div className="user-avatar ms-2 flex-shrink-0">
                {getUserInitials(currentUser.username)}
              </div>
            )}
            {isOwn && !showAvatar && (
              <div style={{ width: '48px' }} className="flex-shrink-0"></div>
            )}
          </div>
        )
      })}
      <div ref={messagesEndRef} />
    </div>
  )
}

export default MessageList
