# apps/core/middleware.py
from django.shortcuts import redirect
from django.urls import resolve

class HotelContextMiddleware:
    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request):
        # List of namespaces or URL names to ignore
        exempt_urls = ['login', 'register', 'activate_user', 'reject_user']
        current_url = resolve(request.path_info).url_name

        if current_url in exempt_urls:
            return self.get_response(request)

        # Your existing logic for hotel context goes here...
        return self.get_response(request)