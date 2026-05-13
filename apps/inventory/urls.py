from django.urls import path
from .views import inventory_list_view, inventory_detail_view

urlpatterns = [
    path('', inventory_list_view, name='inventory_list'),
    path('<int:item_id>/', inventory_detail_view, name='inventory_detail'),
]
