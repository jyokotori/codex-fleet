import { useEffect, useRef, useState, useCallback } from 'react'

interface UseWebSocketOptions {
  onMessage?: (data: string) => void
  onBinaryMessage?: (data: Uint8Array) => void
  onOpen?: () => void
  onClose?: () => void
}

export function useWebSocket(url: string | null, options: UseWebSocketOptions = {}) {
  const wsRef = useRef<WebSocket | null>(null)
  const [isConnected, setIsConnected] = useState(false)

  // Store callbacks in refs so connect() doesn't depend on them
  const onMessageRef = useRef(options.onMessage)
  const onBinaryMessageRef = useRef(options.onBinaryMessage)
  const onOpenRef = useRef(options.onOpen)
  const onCloseRef = useRef(options.onClose)

  onMessageRef.current = options.onMessage
  onBinaryMessageRef.current = options.onBinaryMessage
  onOpenRef.current = options.onOpen
  onCloseRef.current = options.onClose

  const connect = useCallback(() => {
    if (!url) return
    if (wsRef.current?.readyState === WebSocket.OPEN || wsRef.current?.readyState === WebSocket.CONNECTING) return

    const token = localStorage.getItem('token')
    const wsUrl = url.startsWith('ws') ? url : `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}${url}`
    const fullUrl = token ? `${wsUrl}${wsUrl.includes('?') ? '&' : '?'}token=${token}` : wsUrl

    const ws = new WebSocket(fullUrl)
    ws.binaryType = 'arraybuffer'
    wsRef.current = ws

    ws.onopen = () => {
      setIsConnected(true)
      onOpenRef.current?.()
    }

    ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        onBinaryMessageRef.current?.(new Uint8Array(event.data))
      } else {
        onMessageRef.current?.(typeof event.data === 'string' ? event.data : '')
      }
    }

    ws.onclose = () => {
      setIsConnected(false)
      onCloseRef.current?.()
      // No auto-reconnect — caller must explicitly reconnect if needed
    }

    ws.onerror = () => {
      ws.close()
    }
  }, [url])

  const disconnect = useCallback(() => {
    wsRef.current?.close()
    wsRef.current = null
  }, [])

  const send = useCallback((data: string) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(data)
    }
  }, [])

  const sendBinary = useCallback((data: Uint8Array) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(data)
    }
  }, [])

  useEffect(() => {
    connect()
    return () => disconnect()
  }, [connect, disconnect])

  return { isConnected, send, sendBinary, disconnect, connect }
}
