FROM golang:1.27 AS build
WORKDIR /src
COPY . .
RUN CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /pushrelay ./cmd/pushrelay

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /pushrelay /pushrelay
EXPOSE 8080
ENTRYPOINT ["/pushrelay"]
