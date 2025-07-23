import React, { useState, useEffect } from 'react'
import { Modal, Form, Button, ListGroup, Alert, InputGroup } from 'react-bootstrap'

const InviteUserModal = ({ show, onHide, onInviteUser, users, activeGroup }) => {
  const [searchTerm, setSearchTerm] = useState('')
  const [selectedUser, setSelectedUser] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  const filteredUsers = users.filter(user => 
    user.username.toLowerCase().includes(searchTerm.toLowerCase()) ||
    user.email.toLowerCase().includes(searchTerm.toLowerCase())
  )

  const handleInvite = async (e) => {
    e.preventDefault()
    
    if (!selectedUser) {
      setError('Please select a user to invite')
      return
    }

    setLoading(true)
    setError('')

    try {
      await onInviteUser(activeGroup.id, selectedUser)
      setSelectedUser('')
      setSearchTerm('')
      onHide()
    } catch (error) {
      setError(error.message || 'Failed to invite user')
    } finally {
      setLoading(false)
    }
  }

  const handleClose = () => {
    setSelectedUser('')
    setSearchTerm('')
    setError('')
    onHide()
  }

  return (
    <Modal show={show} onHide={handleClose} centered size="lg">
      <Modal.Header closeButton>
        <Modal.Title>
          <i className="bi bi-person-plus me-2"></i>
          Invite User to {activeGroup?.name}
        </Modal.Title>
      </Modal.Header>
      <Form onSubmit={handleInvite}>
        <Modal.Body>
          {error && <Alert variant="danger">{error}</Alert>}
          
          <Form.Group className="mb-3">
            <Form.Label>Search Users</Form.Label>
            <InputGroup>
              <InputGroup.Text>
                <i className="bi bi-search"></i>
              </InputGroup.Text>
              <Form.Control
                type="text"
                placeholder="Search by username or email..."
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
              />
            </InputGroup>
          </Form.Group>

          <Form.Group className="mb-3">
            <Form.Label>Available Users</Form.Label>
            <div style={{ maxHeight: '300px', overflowY: 'auto' }}>
              {filteredUsers.length === 0 ? (
                <div className="text-center text-muted py-4">
                  <i className="bi bi-people display-4 mb-3"></i>
                  <p>No users found</p>
                </div>
              ) : (
                <ListGroup>
                  {filteredUsers.map((user) => (
                    <ListGroup.Item
                      key={user.id}
                      className={`d-flex align-items-center ${selectedUser === user.username ? 'active' : ''}`}
                      style={{ cursor: 'pointer' }}
                      onClick={() => setSelectedUser(user.username)}
                    >
                      <div className="user-avatar me-3">
                        {user.username.substring(0, 2).toUpperCase()}
                      </div>
                      <div className="flex-grow-1">
                        <h6 className="mb-0">{user.username}</h6>
                        <small className={selectedUser === user.username ? 'text-light' : 'text-muted'}>
                          {user.email}
                        </small>
                      </div>
                      <div>
                        {user.is_online && (
                          <span className="badge bg-success">
                            <i className="bi bi-circle-fill me-1"></i>
                            Online
                          </span>
                        )}
                      </div>
                    </ListGroup.Item>
                  ))}
                </ListGroup>
              )}
            </div>
          </Form.Group>

          {selectedUser && (
            <Alert variant="info">
              <i className="bi bi-info-circle me-2"></i>
              You are about to invite <strong>{selectedUser}</strong> to join <strong>{activeGroup?.name}</strong>
            </Alert>
          )}
        </Modal.Body>
        <Modal.Footer>
          <Button variant="secondary" onClick={handleClose}>
            Cancel
          </Button>
          <Button 
            variant="primary" 
            type="submit" 
            disabled={!selectedUser || loading}
          >
            {loading ? (
              <>
                <span className="spinner-border spinner-border-sm me-2" role="status" aria-hidden="true"></span>
                Inviting...
              </>
            ) : (
              <>
                <i className="bi bi-send me-1"></i>
                Send Invite
              </>
            )}
          </Button>
        </Modal.Footer>
      </Form>
    </Modal>
  )
}

export default InviteUserModal
