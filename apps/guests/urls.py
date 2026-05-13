from django.urls import path
from .views import guest_list_view, guest_detail_view

urlpatterns = [
    path('', guest_list_view, name='guest_list'),
    path('<int:guest_id>/', guest_detail_view, name='guest_detail'),
]
