import type { ServerWebSocket } from "bun";
import { Database } from "bun:sqlite";

console.log("📦 Importing required modules...");

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

console.log("📝 Defined types for Client, WebSocketData, and ChatMessage");

const clients: Client[] = [];
console.log("👥 Initialized empty clients array");

console.log("🗄️ Creating SQLite database connection...");
const db = new Database();

// Initialize database
console.log("🔧 Initializing database schema...");
db.run(`
  CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content TEXT NOT NULL,
    authorId TEXT NOT NULL,
    timestamp INTEGER NOT NULL
  )
`);
console.log("✅ Database schema created successfully");

// Insert initial system messages if table is empty
console.log("🔍 Checking if messages table is empty...");
if (!(db.query("SELECT COUNT(*) as count FROM messages").get() as {count: number}).count) {
  console.log("📝 Inserting initial system messages...");
  const initialMessages = [
    ["yo this terminal chat UI is straight fire ngl 🔥", "skibidi_wizard_42", Date.now() - 4000],
    ["fr fr the emoji support be hittin different", "rizz_master_69", Date.now() - 3000],
    ["skibidi_wizard_42 left the chat", "system", Date.now() - 2000],
    ["ong the auto usernames are giving main character energy", "based_demon_55", Date.now() - 1000]
  ];
  
  for (const [content, authorId, timestamp] of initialMessages) {
    console.log(`💾 Inserting message from ${authorId}: ${content}`);
    db.run(`
      INSERT INTO messages (content, authorId, timestamp)
      VALUES (?, ?, ?)
    `, [content, authorId, timestamp as any]);
  }
  console.log("✅ Initial messages inserted successfully");
}

console.log("📚 Fetching recent messages from database...");
const recentMessages = db.query(`
  SELECT content, authorId, timestamp
  FROM messages
  ORDER BY timestamp DESC
  LIMIT 5
`).all().reverse() as unknown as ChatMessage[];
console.log(`📊 Loaded ${recentMessages.length} recent messages`);

const MAX_RECENT_MESSAGES = 5;
console.log(`⚙️ Set maximum recent messages to ${MAX_RECENT_MESSAGES}`);

console.log("\n🚀 Initializing WebSocket server...\n");

const server = Bun.serve<WebSocketData, { userid: any }>({
  port: 3000,

  fetch(req, server) {
    const url = new URL(req.url);
    console.log(`\n📨 Incoming request to: ${url.pathname}`);

    if (url.pathname === "/users") {
      console.log("👥 Users list requested");
      return new Response(
        JSON.stringify({ count: clients.length, users: clients }),
        {
          headers: { "Content-Type": "application/json" },
        }
      );
    }

    if (url.pathname === "/messages") {
      console.log(
        `📜 All messages requested - Total messages: ${recentMessages.length}`
      );
      return new Response(JSON.stringify(recentMessages), {
        headers: { "Content-Type": "application/json" },
      });
    }

    if (url.pathname === "/messages/recent") {
      console.log("📝 Recent messages requested - Returning last 10 messages");
      return new Response(
        JSON.stringify({ messages: recentMessages.slice(0, 10) }),
        {
          headers: { "Content-Type": "application/json" },
        }
      );
    }

    const userId = url.searchParams.get("userId");
    if (!userId) {
      console.log("ℹ️ System info requested");
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
        }
      );
    }

    if (server.upgrade(req, { data: { userId } })) {
      console.log(`🔄 WebSocket upgrade requested for user: ${userId}`);
      return;
    }

    console.error("❌ WebSocket upgrade failed");
    return new Response("WebSocket upgrade failed", { status: 500 });
  },

  websocket: {
    open(ws) {
      clients.push({ ws, id: ws.data.userId });
      console.log(`\n🟢 Client connected: ${ws.data.userId}`);
      console.log(`👥 Total clients: ${clients.length}`);

      console.log(`📤 Sending message history to ${ws.data.userId}`);
      for (const m of recentMessages.slice(0,5)) {
        ws.send(JSON.stringify(m));
      }
      ws.send(
        JSON.stringify({
          content: "History loaded...",
          authorId: "history_loaded",
          timestamp: Date.now(),
        })
      );
      console.log(`📚 Sent message history to: ${ws.data.userId}`);

      const joinMessage: ChatMessage = {
        content: `${ws.data.userId} joined the chat....`,
        authorId: "system",
        timestamp: Date.now(),
      };
      broadcastMessage(joinMessage);
      addToRecentMessages(joinMessage);
      console.log(`🎉 Welcome message sent for: ${ws.data.userId}`);
    },

    message(ws, message) {
      try {
        console.log(`\n📩 Received message from: ${ws.data.userId}`);
        const data = JSON.parse(message.toString());
        console.log(`🔍 Parsed message data: ${JSON.stringify(data)}`);

        if (!data.content) {
          console.warn(`⚠️ Invalid message format from: ${ws.data.userId}`);
          ws.send(JSON.stringify({ error: "Invalid message format" }));
          return;
        }

        const chatMessage: ChatMessage = {
          content: data.content,
          authorId: ws.data.userId,
          timestamp: Date.now(),
        };

        console.log(`📝 Created chat message object: ${JSON.stringify(chatMessage)}`);
        addToRecentMessages(chatMessage);
        broadcastMessage(chatMessage);
        console.log(`📢 Message broadcasted from: ${ws.data.userId}`);
        console.log(`💬 Content: ${data.content}`);
      } catch (error) {
        console.error(
          `❌ Error processing message from ${ws.data.userId}:`,
          error
        );
        ws.send(JSON.stringify({ error: "Failed to process message" }));
      }
    },

    close(ws) {
      const index = clients.findIndex((client) => client.ws === ws);
      if (index !== -1) {
        clients.splice(index, 1);
      }
      console.log(`\n🔴 Client disconnected: ${ws.data.userId}`);
      console.log(`👥 Total clients: ${clients.length}`);

      const leaveMessage: ChatMessage = {
        content: `${ws.data.userId} left the chat`,
        authorId: "system",
        timestamp: Date.now(),
      };
      broadcastMessage(leaveMessage);
      addToRecentMessages(leaveMessage);
      console.log(`👋 Goodbye message sent for: ${ws.data.userId}`);
    },
  },
});

function addToRecentMessages(message: ChatMessage) {
  console.log(`\n📥 Adding new message to database: ${JSON.stringify(message)}`);
  // Insert new message into database
  db.run(`
    INSERT INTO messages (content, authorId, timestamp)
    VALUES (?, ?, ?)
  `, [message.content, message.authorId, message.timestamp]);
  console.log("✅ Message inserted into database");

  // Keep only the latest 5 messages in memory
  console.log("🔄 Updating recent messages in memory");
  const latestMessages = db.query(`
    SELECT content, authorId, timestamp
    FROM messages
    ORDER BY timestamp DESC
    LIMIT 5
  `).all().reverse();
  console.log(`📊 Retrieved ${latestMessages.length} latest messages from database`);

  recentMessages.length = 0;
  recentMessages.push(...latestMessages as ChatMessage[]);
  console.log("✅ Recent messages array updated");

  // Delete old messages from database, keeping only latest 5
  console.log("🗑️ Cleaning up old messages from database");
  db.run(`
    DELETE FROM messages
    WHERE id NOT IN (
      SELECT id FROM messages
      ORDER BY timestamp DESC
      LIMIT 5
    )
  `);
  console.log("✅ Old messages cleaned up");

  console.log(`📝 Message added to history (Total: ${recentMessages.length}/${MAX_RECENT_MESSAGES})`);
}

function broadcastMessage(message: ChatMessage) {
  const messageStr = JSON.stringify(message);
  console.log(`\n📢 Broadcasting message to ${clients.length} clients`);
  console.log(`📦 Message content: ${messageStr}`);
  
  for (const client of clients) {
    client.ws.send(messageStr);
    console.log(`  ↪️ Sent to: ${client.id}`);
  }
  console.log("✅ Broadcast complete");
}

console.log(`\n🌟 WebSocket server running at http://localhost:${server.port}`);
console.log("📡 Ready to accept connections\n");
