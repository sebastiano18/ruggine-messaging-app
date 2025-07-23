import React from 'react'
import { Card, Button, Badge, ListGroup, Toast, ToastContainer } from 'react-bootstrap'

const PendingInvites = ({ invites, onAcceptInvite, onDeclineInvite }) => {
  console.log('🔔 PendingInvites component rendered with invites:', invites)
  
  if (!invites || invites.length === 0) {
    console.log('📭 No pending invites to display')
    return null
  }

  console.log('📬 Displaying', invites.length, 'pending invites')

  const handleAcceptInvite = async (inviteId) => {
    try {
      await onAcceptInvite(inviteId)
    } catch (error) {
      console.error('Failed to accept invite:', error)
    }
  }

  const handleDeclineInvite = async (inviteId) => {
    try {
      await onDeclineInvite(inviteId)
    } catch (error) {
      console.error('Failed to decline invite:', error)
    }
  }

  return (
    <Card className="mb-3">
      <Card.Header className="bg-warning text-dark">
        <i className="bi bi-envelope-fill me-2"></i>
        Inviti Pendenti
        <Badge bg="danger" className="ms-2">
          {invites.length}
        </Badge>
      </Card.Header>
      <Card.Body className="p-0">
        <ListGroup variant="flush">
          {invites.map((invite) => (
            <ListGroup.Item key={invite.id} className="d-flex justify-content-between align-items-center">
              <div>
                <div className="fw-bold">
                  Invito al gruppo: {invite.group_name || 'Gruppo sconosciuto'}
                </div>
                <small className="text-muted">
                  Invitato da: {invite.inviter_name || 'Utente sconosciuto'}
                </small>
              </div>
              
              <div className="btn-group" role="group">
                <Button
                  variant="success"
                  size="sm"
                  onClick={() => handleAcceptInvite(invite.id)}
                  title="Accetta invito"
                >
                  <i className="bi bi-check-lg"></i>
                </Button>
                <Button
                  variant="danger"
                  size="sm"
                  onClick={() => handleDeclineInvite(invite.id)}
                  title="Rifiuta invito"
                >
                  <i className="bi bi-x-lg"></i>
                </Button>
              </div>
            </ListGroup.Item>
          ))}
        </ListGroup>
      </Card.Body>
    </Card>
  )
}

export default PendingInvites
