import { useState, useEffect, useCallback } from 'react'
import WebSocketService from '../services/websocket'

export const useWebSocket = () => {
  const [isConnected, setIsConnected] = useState(false)
  const [messages, setMessages] = useState(new Map()) // groupId -> messages[]
  const [groups, setGroups] = useState([])
  const [users, setUsers] = useState([])
  const [pendingInvites, setPendingInvites] = useState([])

  const connect = useCallback(async () => {
    try {
      await WebSocketService.connect()
      setIsConnected(true)
    } catch (error) {
      console.error('Failed to connect:', error)
      setIsConnected(false)
    }
  }, [])

  const disconnect = useCallback(() => {
    WebSocketService.disconnect()
    setIsConnected(false)
  }, [])

  const sendMessage = useCallback((message) => {
    WebSocketService.send(message)
  }, [])

  const handleServerMessage = useCallback((message) => {
    console.log('Received server message:', message)
    
    switch (message.type) {
      case 'MessageReceived':
        setMessages(prevMessages => {
          const groupMessages = prevMessages.get(message.message.group_id) || []
          const newMessages = new Map(prevMessages)
          newMessages.set(message.message.group_id, [...groupMessages, message.message])
          return newMessages
        })
        break
        
      case 'MessagesHistory':
        setMessages(prevMessages => {
          const newMessages = new Map(prevMessages)
          if (message.messages.length > 0) {
            const groupId = message.messages[0].group_id
            newMessages.set(groupId, message.messages.reverse()) // Reverse to show oldest first
          }
          return newMessages
        })
        break
        
      case 'GroupCreated':
        setGroups(prevGroups => [...prevGroups, message.group])
        break
        
      case 'GroupsList':
        setGroups(message.groups)
        break
        
      case 'UsersList':
        setUsers(message.users)
        break
        
      case 'UserInvited':
        console.log('User invited successfully:', message)
        // Optionally show a notification
        break
        
      case 'InviteReceived':
        console.log('🎉 INVITE RECEIVED!', message)
        console.log('📧 Invite details:', {
          invite: message.invite,
          group_name: message.group_name,
          inviter_name: message.inviter_name
        })
        
        // Temporary alert to confirm receipt
        alert(`🎉 Hai ricevuto un invito per il gruppo "${message.group_name}" da ${message.inviter_name}!`)
        
        setPendingInvites(prevInvites => {
          const newInvite = {
            ...message.invite,
            group_name: message.group_name,
            inviter_name: message.inviter_name
          }
          console.log('💾 Adding invite to pending list:', newInvite)
          const updatedInvites = [...prevInvites, newInvite]
          console.log('📝 Updated pending invites:', updatedInvites)
          return updatedInvites
        })
        // Optionally show a notification
        break
        
      case 'InviteAccepted':
        console.log('Invite accepted:', message)
        // Refresh groups list when user accepts an invite
        sendMessage({ type: 'ListGroups' })
        break
        
      case 'InviteDeclined':
        console.log('Invite declined:', message)
        break
        
      case 'Error':
        console.error('Server error:', message.error)
        break
        
      case 'LoginSuccess':
        console.log('Login successful:', message.user)
        break
        
      case 'LoginFailed':
        console.error('Login failed:', message.error)
        break
        
      case 'RegisterSuccess':
        console.log('Registration successful:', message.user)
        break
        
      case 'RegisterFailed':
        console.error('Registration failed:', message.error)
        break
        
      default:
        console.log('Unhandled message type:', message.type)
    }
  }, [])

  useEffect(() => {
    const listenerId = 'main-app'
    WebSocketService.addListener(listenerId, handleServerMessage)
    
    return () => {
      WebSocketService.removeListener(listenerId)
    }
  }, [handleServerMessage])

  // Monitor connection status
  useEffect(() => {
    const checkConnection = () => {
      setIsConnected(WebSocketService.isConnected())
    }
    
    const interval = setInterval(checkConnection, 1000)
    return () => clearInterval(interval)
  }, [])

  return {
    socket: WebSocketService.socket,
    isConnected,
    messages,
    groups,
    users,
    pendingInvites,
    connect,
    disconnect,
    sendMessage
  }
}
