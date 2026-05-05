from django.contrib import admin
from .models import Guest, GuestDocument

@admin.register(Guest)
class GuestAdmin(admin.ModelAdmin):
    list_display = ('first_name', 'last_name', 'email', 'is_foreigner', 'nationality', 'tc_kimlik_no')
    list_filter = ('is_foreigner', 'nationality')
    search_fields = ('first_name', 'last_name', 'tc_kimlik_no', 'passport_number')

admin.site.register(GuestDocument)
