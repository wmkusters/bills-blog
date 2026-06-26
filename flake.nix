{
  description = "nginx dev server for static site";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    in {
      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            packages = [ pkgs.nginx ];

            shellHook = ''
              mkdir -p /tmp/nginx-dev/{logs,client_body}

              cat > /tmp/nginx-dev/nginx.conf <<EOF
              worker_processes 1;
              error_log /tmp/nginx-dev/logs/error.log;
              pid /tmp/nginx-dev/nginx.pid;

              events { worker_connections 1024; }

              http {
                include       ${pkgs.nginx}/conf/mime.types;
                access_log    /tmp/nginx-dev/logs/access.log;
                client_body_temp_path /tmp/nginx-dev/client_body;

                server {
                  listen 8080;
                  root $PWD/web;
                  index index.html;

                  location / {
                    try_files \$uri \$uri/ /index.html;
                  }
                  location /pkg/ {
                    alias $PWD/pkg/;
                  }
                  location /assets/ {
                    alias $PWD/assets/;
                  }
                }
              }
              EOF

              echo "Starting nginx on http://localhost:8080"
              echo "Serving from: $PWD/web"
              nginx -e /tmp/nginx-dev/logs/error.log -c /tmp/nginx-dev/nginx.conf

              trap "nginx -e /tmp/nginx-dev/logs/error.log -c /tmp/nginx-dev/nginx.conf -s stop" EXIT
            '';
          };
        });
    };
}
