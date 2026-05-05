from django.contrib import admin
from .models import Room, RoomType

@admin.register(Room)
class RoomAdmin(admin.ModelAdmin):
    list_display = ('number', 'room_type', 'status', 'hotel_id')
    list_filter = ('status', 'hotel_id')

@admin.register(RoomType)
class RoomTypeAdmin(admin.ModelAdmin):
    list_display = ('name', 'base_rate', 'hotel_id')
