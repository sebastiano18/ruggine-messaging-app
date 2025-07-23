import React from 'react'
import { Card, ListGroup, Button, Badge, Dropdown } from 'react-bootstrap'
import dayjs from 'dayjs'

const Sidebar = ({ 
  user, 
  groups, 
  activeGroup, 
  isConnected, 
  onGroupSelect, 
  onCreateGroup, 
  onLogout 
}) => {
  const getUserInitials = (username) => {
    return username.substring(0, 2).toUpperCase()
  }

  return (
    <div className="h-100 d-flex flex-column">
      {/* User Profile Section */}
      <Card className="border-0 border-bottom rounded-0">
        <Card.Body className="py-3">
          <div className="d-flex align-items-center justify-content-between">
            <div className="d-flex align-items-center">
              <div className="user-avatar me-3">
                {getUserInitials(user.username)}
              </div>
              <div>
                <h6 className="mb-0">{user.username}</h6>
                <small className="text-muted d-flex align-items-center">
                  <i className={`bi bi-circle-fill me-1 ${isConnected ? 'text-success' : 'text-danger'}`} style={{fontSize: '8px'}}></i>
                  {isConnected ? 'Online' : 'Offline'}
                </small>
              </div>
            </div>
            <Dropdown>
              <Dropdown.Toggle variant="light" size="sm" className="border-0">
                <i className="bi bi-three-dots-vertical"></i>
              </Dropdown.Toggle>
              <Dropdown.Menu>
                <Dropdown.Item onClick={onLogout}>
                  <i className="bi bi-box-arrow-right me-2"></i>
                  Logout
                </Dropdown.Item>
              </Dropdown.Menu>
            </Dropdown>
          </div>
        </Card.Body>
      </Card>

      {/* Groups Section */}
      <div className="flex-grow-1 d-flex flex-column">
        <Card className="border-0 flex-grow-1">
          <Card.Header className="bg-transparent border-0 d-flex justify-content-between align-items-center">
            <h6 className="mb-0 fw-bold">Groups</h6>
            <Button 
              variant="primary" 
              size="sm" 
              onClick={onCreateGroup}
              disabled={!isConnected}
            >
              <i className="bi bi-plus"></i>
            </Button>
          </Card.Header>
          <Card.Body className="p-0 flex-grow-1" style={{overflowY: 'auto'}}>
            {groups.length === 0 ? (
              <div className="text-center p-4 text-muted">
                <i className="bi bi-chat-dots display-4 mb-3"></i>
                <p>No groups yet</p>
                <Button 
                  variant="outline-primary" 
                  size="sm" 
                  onClick={onCreateGroup}
                  disabled={!isConnected}
                >
                  Create your first group
                </Button>
              </div>
            ) : (
              <ListGroup variant="flush">
                {groups.map((group) => (
                  <ListGroup.Item
                    key={group.id}
                    className={`group-item border-0 ${activeGroup?.id === group.id ? 'active' : ''}`}
                    onClick={() => onGroupSelect(group)}
                    style={{ cursor: 'pointer' }}
                  >
                    <div className="d-flex justify-content-between align-items-center">
                      <div>
                        <div className="d-flex align-items-center">
                          <i className={`bi ${group.is_private ? 'bi-lock-fill' : 'bi-people-fill'} me-2`}></i>
                          <h6 className="mb-0">{group.name}</h6>
                        </div>
                        {group.description && (
                          <small className={`${activeGroup?.id === group.id ? 'text-light' : 'text-muted'}`}>
                            {group.description}
                          </small>
                        )}
                      </div>
                      <div className="text-end">
                        <small className={`${activeGroup?.id === group.id ? 'text-light' : 'text-muted'}`}>
                          {dayjs(group.created_at).format('MMM D')}
                        </small>
                      </div>
                    </div>
                  </ListGroup.Item>
                ))}
              </ListGroup>
            )}
          </Card.Body>
        </Card>
      </div>

      {/* Connection Status */}
      <Card className="border-0 border-top rounded-0">
        <Card.Body className="py-2">
          <div className="text-center">
            <small className={`${isConnected ? 'text-success' : 'text-danger'}`}>
              <i className={`bi ${isConnected ? 'bi-wifi' : 'bi-wifi-off'} me-1`}></i>
              {isConnected ? 'Connected to server' : 'Disconnected'}
            </small>
          </div>
        </Card.Body>
      </Card>
    </div>
  )
}

export default Sidebar
