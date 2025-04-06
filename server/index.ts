import type { ServerWebSocket } from "bun";

type Client = {
  ws: ServerWebSocket<WebSocketData>;
  id: string;
};

type WebSocketData = {
  userId: string;
};

type ChatMessage = {
  content: string;
  authorId: string;
  timestamp: number;
};

const clients: Client[] = [];

const recentMessages: ChatMessage[] = [];
const MAX_RECENT_MESSAGES = 50;

const server = Bun.serve<WebSocketData, { userid: any }>({
  port: 3000,

  fetch(req, server) {
    const url = new URL(req.url);

    if (url.pathname === "/users") {
      return new Response(
        JSON.stringify({ count: clients.length, users: clients }),
        {
          headers: { "Content-Type": "application/json" },
        },
      );
    }

    if (url.pathname === "/messages") {
      return new Response(JSON.stringify({ messages: recentMessages }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    if (url.pathname === "/messages/recent") {
      return new Response(
        JSON.stringify({ messages: recentMessages.slice(0, 10) }),
        {
          headers: { "Content-Type": "application/json" },
        },
      );
    }

    const userId = url.searchParams.get("userId");
    if (!userId) {
      return new Response(
        JSON.stringify({
          system: {
            platform: process.platform,
            arch: process.arch,
            version: process.version,
            memoryUsage: process.memoryUsage(),
            cpuUsage: process.cpuUsage(),
            uptime: process.uptime(),
          },
          chat: {
            connectedClients: clients.length,
            messageCount: recentMessages.length,
            maxMessages: MAX_RECENT_MESSAGES,
          },
          process: {
            pid: process.pid,
            ppid: process.ppid,
            title: process.title,
            cwd: process.cwd(),
          },
        }),
        {
          headers: {
            "Content-Type": "application/json",
            "Cache-Control": "no-cache",
          },
        },
      );
    }

    if (server.upgrade(req, { data: { userId } })) {
      return;
    }

    return new Response("WebSocket upgrade failed", { status: 500 });
  },

  websocket: {
    open(ws) {
      clients.push({ ws, id: ws.data.userId });
      console.log(`Client connected. Total clients: ${clients.length}`);

      ws.send(JSON.stringify({ type: "history", messages: recentMessages }));

      const joinMessage: ChatMessage = {
        content: `${ws.data.userId} joined the chat....`,
        authorId: "system",
        timestamp: Date.now(),
      };
      broadcastMessage(joinMessage);
      addToRecentMessages(joinMessage);
    },

    message(ws, message) {
      try {
        const data = JSON.parse(message.toString());

        if (!data.content) {
          ws.send(JSON.stringify({ error: "Invalid message format" }));
          return;
        }

        const chatMessage: ChatMessage = {
          content: data.content,
          authorId: ws.data.userId,
          timestamp: Date.now(),
        };

        addToRecentMessages(chatMessage);
        broadcastMessage(chatMessage);
      } catch (error) {
        console.error("Error processing message:", error);
        ws.send(JSON.stringify({ error: "Failed to process message" }));
      }
    },

    close(ws) {
      const index = clients.findIndex((client) => client.ws === ws);
      if (index !== -1) {
        clients.splice(index, 1);
      }
      console.log(`Client disconnected. Total clients: ${clients.length}`);

      const leaveMessage: ChatMessage = {
        content: `${ws.data.userId} left the chat`,
        authorId: "system",
        timestamp: Date.now(),
      };
      broadcastMessage(leaveMessage);
      addToRecentMessages(leaveMessage);
    },
  },
});

function addToRecentMessages(message: ChatMessage) {
  recentMessages.push(message);
  if (recentMessages.length > MAX_RECENT_MESSAGES) {
    recentMessages.shift();
  }
}

function broadcastMessage(message: ChatMessage) {
  const messageStr = JSON.stringify(message);
  for (const client of clients) {
    client.ws.send(messageStr);
  }
}

console.log(`WebSocket server running at http://localhost:${server.port}`);
