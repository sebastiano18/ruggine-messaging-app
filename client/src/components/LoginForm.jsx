import React, { useState } from 'react'
import { Container, Row, Col, Card, Form, Button, Alert, Tab, Tabs } from 'react-bootstrap'
import WebSocketService from '../services/websocket'

const LoginForm = ({ onLogin, onSendMessage, isConnected }) => {
  const [activeTab, setActiveTab] = useState('login')
  const [formData, setFormData] = useState({
    username: '',
    email: '',
    password: '',
    confirmPassword: ''
  })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState('')

  const handleInputChange = (e) => {
    setFormData({
      ...formData,
      [e.target.name]: e.target.value
    })
    setError('')
    setSuccess('')
  }

  const handleLogin = async (e) => {
    e.preventDefault()
    setLoading(true)
    setError('')

    if (!isConnected) {
      setError('Not connected to server')
      setLoading(false)
      return
    }

    try {
      // Set up one-time listeners for login response
      const loginPromise = new Promise((resolve, reject) => {
        const handleLoginResponse = (message) => {
          if (message.type === 'LoginSuccess') {
            WebSocketService.removeListener('login-handler')
            resolve(message)
          } else if (message.type === 'LoginFailed') {
            WebSocketService.removeListener('login-handler')
            reject(new Error(message.error))
          }
        }
        
        WebSocketService.addListener('login-handler', handleLoginResponse)
      })

      // Send login message using the shared connection
      onSendMessage({
        type: 'Login',
        username: formData.username,
        password: formData.password
      })

      // Wait for response
      const response = await Promise.race([
        loginPromise,
        new Promise((_, reject) => 
          setTimeout(() => reject(new Error('Login timeout')), 10000)
        )
      ])

      onLogin(response.user, response.token)
    } catch (error) {
      setError(error.message || 'Login failed')
    } finally {
      setLoading(false)
    }
  }

  const handleRegister = async (e) => {
    e.preventDefault()
    setLoading(true)
    setError('')
    setSuccess('')

    if (formData.password !== formData.confirmPassword) {
      setError('Passwords do not match')
      setLoading(false)
      return
    }

    if (!isConnected) {
      setError('Not connected to server')
      setLoading(false)
      return
    }

    try {
      // Set up one-time listeners for register response
      const registerPromise = new Promise((resolve, reject) => {
        const handleRegisterResponse = (message) => {
          if (message.type === 'RegisterSuccess') {
            WebSocketService.removeListener('register-handler')
            resolve(message)
          } else if (message.type === 'RegisterFailed') {
            WebSocketService.removeListener('register-handler')
            reject(new Error(message.error))
          }
        }
        
        WebSocketService.addListener('register-handler', handleRegisterResponse)
      })

      // Send register message using the shared connection
      onSendMessage({
        type: 'Register',
        username: formData.username,
        email: formData.email,
        password: formData.password
      })

      // Wait for response
      await Promise.race([
        registerPromise,
        new Promise((_, reject) => 
          setTimeout(() => reject(new Error('Registration timeout')), 10000)
        )
      ])

      setSuccess('Registration successful! Please login.')
      setActiveTab('login')
      setFormData({
        username: formData.username, // Keep username for easy login
        email: '',
        password: '',
        confirmPassword: ''
      })
    } catch (error) {
      setError(error.message || 'Registration failed')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="login-container">
      <Container>
        <Row className="justify-content-center">
          <Col md={6} lg={4}>
            <Card className="login-card">
              <Card.Body>
                <div className="text-center mb-4">
                  <h1 className="h3 mb-3 fw-normal">
                    <i className="bi bi-chat-dots-fill me-2"></i>
                    Ruggine
                  </h1>
                  <p className="text-muted">Multi-platform Chat Application</p>
                </div>

                {error && <Alert variant="danger">{error}</Alert>}
                {success && <Alert variant="success">{success}</Alert>}

                <Tabs 
                  activeKey={activeTab} 
                  onSelect={(k) => setActiveTab(k)}
                  className="mb-3"
                  justify
                >
                  <Tab eventKey="login" title="Login">
                    <Form onSubmit={handleLogin}>
                      <Form.Group className="mb-3">
                        <Form.Label>Username</Form.Label>
                        <Form.Control
                          type="text"
                          name="username"
                          value={formData.username}
                          onChange={handleInputChange}
                          required
                          placeholder="Enter your username"
                        />
                      </Form.Group>

                      <Form.Group className="mb-3">
                        <Form.Label>Password</Form.Label>
                        <Form.Control
                          type="password"
                          name="password"
                          value={formData.password}
                          onChange={handleInputChange}
                          required
                          placeholder="Enter your password"
                        />
                      </Form.Group>

                      <Button 
                        variant="primary" 
                        type="submit" 
                        className="w-100"
                        disabled={loading}
                      >
                        {loading ? (
                          <>
                            <span className="spinner-border spinner-border-sm me-2" role="status" aria-hidden="true"></span>
                            Signing in...
                          </>
                        ) : (
                          'Sign In'
                        )}
                      </Button>
                    </Form>
                  </Tab>

                  <Tab eventKey="register" title="Register">
                    <Form onSubmit={handleRegister}>
                      <Form.Group className="mb-3">
                        <Form.Label>Username</Form.Label>
                        <Form.Control
                          type="text"
                          name="username"
                          value={formData.username}
                          onChange={handleInputChange}
                          required
                          placeholder="Choose a username"
                        />
                      </Form.Group>

                      <Form.Group className="mb-3">
                        <Form.Label>Email</Form.Label>
                        <Form.Control
                          type="email"
                          name="email"
                          value={formData.email}
                          onChange={handleInputChange}
                          required
                          placeholder="Enter your email"
                        />
                      </Form.Group>

                      <Form.Group className="mb-3">
                        <Form.Label>Password</Form.Label>
                        <Form.Control
                          type="password"
                          name="password"
                          value={formData.password}
                          onChange={handleInputChange}
                          required
                          placeholder="Choose a password"
                        />
                      </Form.Group>

                      <Form.Group className="mb-3">
                        <Form.Label>Confirm Password</Form.Label>
                        <Form.Control
                          type="password"
                          name="confirmPassword"
                          value={formData.confirmPassword}
                          onChange={handleInputChange}
                          required
                          placeholder="Confirm your password"
                        />
                      </Form.Group>

                      <Button 
                        variant="success" 
                        type="submit" 
                        className="w-100"
                        disabled={loading}
                      >
                        {loading ? (
                          <>
                            <span className="spinner-border spinner-border-sm me-2" role="status" aria-hidden="true"></span>
                            Creating account...
                          </>
                        ) : (
                          'Create Account'
                        )}
                      </Button>
                    </Form>
                  </Tab>
                </Tabs>

                <div className="text-center mt-3">
                  <small className="text-muted">
                    Ruggine Chat - Secure Multi-platform Messaging
                  </small>
                </div>
              </Card.Body>
            </Card>
          </Col>
        </Row>
      </Container>
    </div>
  )
}

export default LoginForm
