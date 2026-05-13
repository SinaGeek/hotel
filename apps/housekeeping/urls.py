from django.urls import path
from .views import housekeeping_overview_view, update_room_status, housekeeping_detail_view

urlpatterns = [
    path('', housekeeping_overview_view, name='housekeeping_overview'),
    path('room/<int:room_id>/', housekeeping_detail_view, name='housekeeping_detail'),
    path('update/<int:room_id>/', update_room_status, name='update_room_status'), # Deprecated but keeping for compatibility
]
