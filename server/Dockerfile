FROM oven/bun:1
WORKDIR /server
COPY . .
RUN bun install

ARG PORT
EXPOSE ${PORT:-3000}

CMD ["bun", "index.ts"]