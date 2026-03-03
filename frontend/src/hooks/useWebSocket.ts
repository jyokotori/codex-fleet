import { useEffect, useRef, useState, useCallback } from 'react'

interface UseWebSocketOptions {
  onMessage?: (data: string) => void
  onOpen?: () => void
  onClose?: () => void
  reconnectDelay?: number
  maxReconnects?: number
}

export function useWebSocket(url: string | null, options: UseWebSocketOptions = {}) {
  const { onMessage, onOpen, onClose, reconnectDelay = 3000, maxReconnects = 5 } = options
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectCount = useRef(0)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [isConnected, setIsConnected] = useState(false)

  const connect = useCallback(() => {
    if (!url) return
    if (wsRef.current?.readyState === WebSocket.OPEN) return

    const token = localStorage.getItem('token')
    const wsUrl = url.startsWith('ws') ? url : `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}${url}`
    const fullUrl = token ? `${wsUrl}?token=${token}` : wsUrl

    const ws = new WebSocket(fullUrl)
    wsRef.current = ws

    ws.onopen = () => {
      setIsConnected(true)
      reconnectCount.current = 0
      onOpen?.()
    }

    ws.onmessage = (event) => {
      onMessage?.(typeof event.data === 'string' ? event.data : '')
    }

    ws.onclose = () => {
      setIsConnected(false)
      onClose?.()
      if (reconnectCount.current < maxReconnects) {
        reconnectCount.current++
        reconnectTimer.current = setTimeout(connect, reconnectDelay)
      }
    }

    ws.onerror = () => {
      ws.close()
    }
  }, [url, onMessage, onOpen, onClose, reconnectDelay, maxReconnects])

  const disconnect = useCallback(() => {
    if (reconnectTimer.current) {
      clearTimeout(reconnectTimer.current)
    }
    wsRef.current?.close()
    wsRef.current = null
  }, [])

  const send = useCallback((data: string) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(data)
    }
  }, [])

  useEffect(() => {
    connect()
    return () => disconnect()
  }, [connect, disconnect])

  return { isConnected, send, disconnect, connect }
}
