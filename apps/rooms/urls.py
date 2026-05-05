from django.urls import path
from . import views

urlpatterns = [
    path('overview/', views.room_overview_view, name='room_overview'),
    path('room/<int:room_id>/', views.room_detail_view, name='room_detail'),
    path('api/guest-search/', views.guest_search_view, name='guest_search'),
    path('api/context-action/', views.context_menu_action_view, name='context_action'),
]
