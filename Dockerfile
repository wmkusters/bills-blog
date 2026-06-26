FROM nginx:alpine
COPY ./web /usr/share/nginx/html
COPY ./pkg /usr/share/nginx/html/pkg
COPY ./assets /usr/share/nginx/html/assets
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
