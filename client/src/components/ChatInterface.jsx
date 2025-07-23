import React, { useState, useEffect } from 'react'
import { Row, Col } from 'react-bootstrap'
import Sidebar from './Sidebar'
import ChatWindow from './ChatWindow'
import CreateGroupModal from './CreateGroupModal'
import InviteUserModal from './InviteUserModal'
import PendingInvites from './PendingInvites'

const ChatInterface = ({ 
  user, 
  isConnected, 
  messages, 
  groups, 
  users, 
  pendingInvites,
  onSendMessage, 
  onLogout 
}) => {
  console.log('🎛️ ChatInterface rendered with:', {
    user: user?.username,
    isConnected,
    groupsCount: groups?.length,
    usersCount: users?.length,
    pendingInvitesCount: pendingInvites?.length,
    pendingInvites
  })
  const [activeGroup, setActiveGroup] = useState(null)
  const [showCreateGroup, setShowCreateGroup] = useState(false)
  const [showInviteUser, setShowInviteUser] = useState(false)

  // Auto-select first group when groups load
  useEffect(() => {
    if (groups.length > 0 && !activeGroup) {
      setActiveGroup(groups[0])
      // Load messages for the first group
      onSendMessage({
        type: 'GetMessages',
        group_id: groups[0].id,
        limit: 50,
        offset: 0
      })
    }
  }, [groups, activeGroup, onSendMessage])

  // Load users when component mounts
  useEffect(() => {
    if (isConnected && users.length === 0) {
      onSendMessage({
        type: 'GetUsers'
      })
    }
  }, [isConnected, users.length, onSendMessage])

  const handleGroupSelect = (group) => {
    setActiveGroup(group)
    // Load messages for selected group
    onSendMessage({
      type: 'GetMessages',
      group_id: group.id,
      limit: 50,
      offset: 0
    })
  }

  const handleCreateGroup = (groupData) => {
    onSendMessage({
      type: 'CreateGroup',
      name: groupData.name,
      description: groupData.description,
      is_private: groupData.isPrivate
    })
    setShowCreateGroup(false)
  }

  const handleInviteUser = async (groupId, username) => {
    try {
      // Send the invite message
      onSendMessage({
        type: 'InviteToGroup',
        group_id: groupId,
        username: username
      })
      
      // Return success - we'll rely on server response for error handling
      return Promise.resolve({ success: true })
    } catch (error) {
      return Promise.reject(error)
    }
  }

  const handleAcceptInvite = async (inviteId) => {
    onSendMessage({
      type: 'AcceptInvite',
      invite_id: inviteId
    })
  }

  const handleDeclineInvite = async (inviteId) => {
    onSendMessage({
      type: 'DeclineInvite',
      invite_id: inviteId
    })
  }

  const handleSendMessage = (content) => {
    if (activeGroup && content.trim()) {
      onSendMessage({
        type: 'SendMessage',
        group_id: activeGroup.id,
        content: content.trim()
      })
    }
  }

  const currentMessages = activeGroup ? messages.get(activeGroup.id) || [] : []

  return (
    <>
      <Row className="h-100 g-0">
        <Col md={3} className="sidebar">
          <Sidebar
            user={user}
            groups={groups}
            activeGroup={activeGroup}
            isConnected={isConnected}
            onGroupSelect={handleGroupSelect}
            onCreateGroup={() => setShowCreateGroup(true)}
            onLogout={onLogout}
          />
          
          {/* Inviti pendenti nella sidebar */}
          <div className="p-3">
            <div className="mb-2">
              <small className="text-muted">
                Debug: {pendingInvites ? pendingInvites.length : 0} inviti pendenti
              </small>
            </div>
            {pendingInvites && pendingInvites.length > 0 ? (
              <PendingInvites
                invites={pendingInvites}
                onAcceptInvite={handleAcceptInvite}
                onDeclineInvite={handleDeclineInvite}
              />
            ) : (
              <div className="text-muted small">
                Nessun invito pendente
              </div>
            )}
          </div>
        </Col>
        <Col md={9}>
          <ChatWindow
            user={user}
            activeGroup={activeGroup}
            messages={currentMessages}
            onSendMessage={handleSendMessage}
            onInviteUser={() => setShowInviteUser(true)}
            isConnected={isConnected}
          />
        </Col>
      </Row>

      <CreateGroupModal
        show={showCreateGroup}
        onHide={() => setShowCreateGroup(false)}
        onCreateGroup={handleCreateGroup}
      />

      <InviteUserModal
        show={showInviteUser}
        onHide={() => setShowInviteUser(false)}
        onInviteUser={handleInviteUser}
        users={users}
        activeGroup={activeGroup}
      />
    </>
  )
}

export default ChatInterface
