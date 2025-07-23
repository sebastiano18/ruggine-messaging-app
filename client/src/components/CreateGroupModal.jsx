import React, { useState } from 'react'
import { Modal, Form, Button, Alert } from 'react-bootstrap'

const CreateGroupModal = ({ show, onHide, onCreateGroup }) => {
  const [formData, setFormData] = useState({
    name: '',
    description: '',
    isPrivate: false
  })
  const [error, setError] = useState('')

  const handleInputChange = (e) => {
    const { name, value, type, checked } = e.target
    setFormData({
      ...formData,
      [name]: type === 'checkbox' ? checked : value
    })
    setError('')
  }

  const handleSubmit = (e) => {
    e.preventDefault()
    
    if (!formData.name.trim()) {
      setError('Group name is required')
      return
    }

    if (formData.name.length < 3) {
      setError('Group name must be at least 3 characters long')
      return
    }

    console.log('Creating group with data:', {
      name: formData.name.trim(),
      description: formData.description.trim() || null,
      isPrivate: formData.isPrivate
    })

    onCreateGroup({
      name: formData.name.trim(),
      description: formData.description.trim() || null,
      isPrivate: formData.isPrivate
    })

    // Reset form
    setFormData({
      name: '',
      description: '',
      isPrivate: false
    })
    setError('')
  }

  const handleClose = () => {
    setFormData({
      name: '',
      description: '',
      isPrivate: false
    })
    setError('')
    onHide()
  }

  return (
    <Modal show={show} onHide={handleClose} centered>
      <Modal.Header closeButton>
        <Modal.Title>
          <i className="bi bi-plus-circle me-2"></i>
          Create New Group
        </Modal.Title>
      </Modal.Header>
      <Form onSubmit={handleSubmit}>
        <Modal.Body>
          {error && <Alert variant="danger">{error}</Alert>}
          
          <Form.Group className="mb-3">
            <Form.Label>Group Name *</Form.Label>
            <Form.Control
              type="text"
              name="name"
              value={formData.name}
              onChange={handleInputChange}
              placeholder="Enter group name"
              required
              maxLength={50}
            />
            <Form.Text className="text-muted">
              Choose a descriptive name for your group (3-50 characters)
            </Form.Text>
          </Form.Group>

          <Form.Group className="mb-3">
            <Form.Label>Description</Form.Label>
            <Form.Control
              as="textarea"
              rows={3}
              name="description"
              value={formData.description}
              onChange={handleInputChange}
              placeholder="Optional: Describe what this group is about"
              maxLength={200}
            />
            <Form.Text className="text-muted">
              Optional description (max 200 characters)
            </Form.Text>
          </Form.Group>

          <Form.Group className="mb-3">
            <Form.Check
              type="checkbox"
              name="isPrivate"
              checked={formData.isPrivate}
              onChange={handleInputChange}
              label={
                <span>
                  <i className="bi bi-lock-fill me-1"></i>
                  Private Group
                </span>
              }
            />
            <Form.Text className="text-muted">
              Private groups require an invitation to join
            </Form.Text>
          </Form.Group>
        </Modal.Body>
        <Modal.Footer>
          <Button variant="secondary" onClick={handleClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit">
            <i className="bi bi-plus me-1"></i>
            Create Group
          </Button>
        </Modal.Footer>
      </Form>
    </Modal>
  )
}

export default CreateGroupModal
